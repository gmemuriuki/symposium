//! Daily admission and lookup for plugin-hook aggregates.

use std::{
    collections::{
        BTreeMap,
        btree_map::{Entry, OccupiedEntry, VacantEntry},
    },
    fmt,
};

use super::{
    AdmittedPluginBucket, PluginHookAggregateSelectionError, PluginHookAggregateState,
    PluginHookMetricsKey, SelectedPluginHookAggregate,
};
use crate::telemetry::{
    schema::{HookAgent, HookSurface, PluginHookAttribution, UtcDay},
    state::{
        BoundRecordingObservation, DayBeforeCurrent,
        open_day::OpenDayUpdate,
        public_row_budget::{DailyPublicRowBudget, PublicRowAdmission},
    },
    storage::metrics::PublicAggregateRecoveryIndex,
};

/// Daily private state for plugin-hook aggregate admission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::telemetry) struct PluginHookAggregateStore {
    public_rows: DailyPublicRowBudget,
    entries: BTreeMap<PluginHookMetricsKey, PluginHookAggregateState>,
}

impl PluginHookAggregateStore {
    #[must_use]
    pub(in crate::telemetry) const fn new(day: UtcDay) -> Self {
        Self {
            public_rows: DailyPublicRowBudget::new(day),
            entries: BTreeMap::new(),
        }
    }

    /// Select or admit the aggregate for one plugin-hook attempt.
    ///
    /// The required recovery index proves that the day's snapshot loaded
    /// successfully. Every surviving public row contributes to the reconciled
    /// allowance, while only a complete current-epoch identity can restore a
    /// missing entry's event identifier. Adoption happens before overflow and
    /// does not spend another slot. A forward UTC-day transition clears the
    /// previous entries before applying the new day's surviving count.
    ///
    /// # Errors
    ///
    /// Returns an admission error if snapshot and observation days disagree,
    /// the observation day moves backward, or stored private state does not
    /// match its map key.
    pub(in crate::telemetry) fn select(
        &mut self,
        recovery: &PublicAggregateRecoveryIndex,
        recording: &BoundRecordingObservation<'_>,
        agent: HookAgent,
        surface: HookSurface,
        attribution: PluginHookAttribution,
    ) -> Result<SelectedPluginHookAggregate<'_>, PluginHookAdmissionError> {
        if recovery.day() != recording.day() {
            return Err(PluginHookAdmissionError::SnapshotDayMismatch {
                snapshot_day: recovery.day(),
                observation_day: recording.day(),
            });
        }
        if self
            .public_rows
            .reconcile(recording.day(), recovery.plugin_hook_public_rows())?
            == OpenDayUpdate::Advanced
        {
            self.entries.clear();
        }

        let bucket = AdmittedPluginBucket::from_attribution(recording, attribution);
        let key = PluginHookMetricsKey::new(recording, agent, surface, &bucket);
        // Returning a selection keeps the mutable map borrow alive. Check
        // first, then perform the borrowing lookup in a separate helper so
        // the absent path can still insert on stable Rust.
        if self.entries.contains_key(&key) {
            return Self::select_existing(&mut self.entries, &key);
        }

        if let Some((plugin, subject)) = bucket.public_identity() {
            if let Some(event_id) = recovery.plugin_hook_event_id(agent, surface, plugin, subject) {
                return Self::insert_recovered(&mut self.entries, key, bucket, event_id);
            }

            if self.public_rows.is_exhausted() {
                return Self::select_public_at_capacity(&mut self.entries, key);
            }

            let PublicRowAdmission::Public = self.public_rows.admit_new() else {
                unreachable!("BUG: a non-exhausted public-row budget must admit a row");
            };
        }

        Self::insert_new(&mut self.entries, key, bucket)
    }

    fn select_public_at_capacity(
        entries: &mut BTreeMap<PluginHookMetricsKey, PluginHookAggregateState>,
        mut key: PluginHookMetricsKey,
    ) -> Result<SelectedPluginHookAggregate<'_>, PluginHookAdmissionError> {
        let bucket = AdmittedPluginBucket::overflow();
        key.bucket = bucket.key();
        Self::select_or_insert(entries, key, bucket)
    }

    fn select_or_insert(
        entries: &mut BTreeMap<PluginHookMetricsKey, PluginHookAggregateState>,
        key: PluginHookMetricsKey,
        bucket: AdmittedPluginBucket,
    ) -> Result<SelectedPluginHookAggregate<'_>, PluginHookAdmissionError> {
        match entries.entry(key) {
            Entry::Occupied(entry) => Self::select_occupied(entry),
            Entry::Vacant(entry) => Self::insert_vacant(entry, bucket),
        }
    }

    fn select_existing<'a>(
        entries: &'a mut BTreeMap<PluginHookMetricsKey, PluginHookAggregateState>,
        key: &PluginHookMetricsKey,
    ) -> Result<SelectedPluginHookAggregate<'a>, PluginHookAdmissionError> {
        let Some(entry) = entries.get_mut(key) else {
            return Err(PluginHookAdmissionError::PrivateState(
                PluginHookAggregateSelectionError,
            ));
        };
        entry
            .select(key)
            .map_err(PluginHookAdmissionError::PrivateState)
    }

    fn insert_new(
        entries: &mut BTreeMap<PluginHookMetricsKey, PluginHookAggregateState>,
        key: PluginHookMetricsKey,
        bucket: AdmittedPluginBucket,
    ) -> Result<SelectedPluginHookAggregate<'_>, PluginHookAdmissionError> {
        let Entry::Vacant(entry) = entries.entry(key) else {
            return Err(PluginHookAdmissionError::PrivateState(
                PluginHookAggregateSelectionError,
            ));
        };
        Self::insert_vacant(entry, bucket)
    }

    fn insert_recovered(
        entries: &mut BTreeMap<PluginHookMetricsKey, PluginHookAggregateState>,
        key: PluginHookMetricsKey,
        bucket: AdmittedPluginBucket,
        event_id: crate::telemetry::schema::EventId,
    ) -> Result<SelectedPluginHookAggregate<'_>, PluginHookAdmissionError> {
        let Entry::Vacant(entry) = entries.entry(key) else {
            return Err(PluginHookAdmissionError::PrivateState(
                PluginHookAggregateSelectionError,
            ));
        };
        let key = entry.key().clone();
        entry
            .insert(PluginHookAggregateState::recovered(&key, bucket, event_id))
            .select(&key)
            .map_err(PluginHookAdmissionError::PrivateState)
    }

    fn select_occupied(
        entry: OccupiedEntry<'_, PluginHookMetricsKey, PluginHookAggregateState>,
    ) -> Result<SelectedPluginHookAggregate<'_>, PluginHookAdmissionError> {
        let key = entry.key().clone();
        entry
            .into_mut()
            .select(&key)
            .map_err(PluginHookAdmissionError::PrivateState)
    }

    fn insert_vacant(
        entry: VacantEntry<'_, PluginHookMetricsKey, PluginHookAggregateState>,
        bucket: AdmittedPluginBucket,
    ) -> Result<SelectedPluginHookAggregate<'_>, PluginHookAdmissionError> {
        let state = PluginHookAggregateState::new(entry.key(), bucket);
        let key = entry.key().clone();
        entry
            .insert(state)
            .select(&key)
            .map_err(PluginHookAdmissionError::PrivateState)
    }

    /// Remove entries from the previous identifier epoch without restoring
    /// the current day's public-row allowance.
    pub(in crate::telemetry) fn reset_identifier_epoch(&mut self) {
        self.entries.clear();
    }

    /// Remove current rows and restore the current day's public-row allowance.
    pub(in crate::telemetry) fn clear(&mut self) {
        self.entries.clear();
        self.public_rows.clear();
    }

    #[cfg(test)]
    const fn admitted_public_rows(&self) -> u64 {
        self.public_rows.admitted()
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.entries.len()
    }
}

/// Failure while selecting private plugin-hook aggregate state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::telemetry) enum PluginHookAdmissionError {
    SnapshotDayMismatch {
        snapshot_day: UtcDay,
        observation_day: UtcDay,
    },
    DayBeforeCurrent(DayBeforeCurrent),
    PrivateState(PluginHookAggregateSelectionError),
}

impl From<DayBeforeCurrent> for PluginHookAdmissionError {
    fn from(error: DayBeforeCurrent) -> Self {
        Self::DayBeforeCurrent(error)
    }
}

impl fmt::Display for PluginHookAdmissionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SnapshotDayMismatch {
                snapshot_day,
                observation_day,
            } => write!(
                formatter,
                "plugin-hook snapshot belongs to {snapshot_day}, not observation day {observation_day}"
            ),
            Self::DayBeforeCurrent(error) => write!(formatter, "plugin-hook admission {error}"),
            Self::PrivateState(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for PluginHookAdmissionError {}

#[cfg(test)]
mod tests;
