//! Daily admission and lookup for plugin-hook aggregates.

use std::{collections::BTreeMap, fmt};

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
        staged_entries::StagedEntries,
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

    /// Stage private-state edits for one hook recording operation.
    ///
    /// The required recovery index proves that the day's snapshot loaded
    /// successfully. Every surviving public row contributes to the reconciled
    /// allowance. Day selection and allowance reconciliation happen on a copy,
    /// so dropping the returned stage leaves this store unchanged. The
    /// invocation coordinator will become the sole caller and committer once
    /// it lands; this per-store entry point remains only for the staged rollout.
    ///
    /// # Errors
    ///
    /// Returns an admission error if snapshot and observation days disagree,
    /// the observation day moves backward, or stored private state does not
    /// match its map key.
    pub(in crate::telemetry) fn stage<'store, 'context, 'identity>(
        &'store mut self,
        recovery: &'context PublicAggregateRecoveryIndex,
        recording: &'context BoundRecordingObservation<'identity>,
    ) -> Result<PluginHookAggregateStage<'store, 'context, 'identity>, PluginHookAdmissionError>
    {
        if recovery.day() != recording.day() {
            return Err(PluginHookAdmissionError::SnapshotDayMismatch {
                snapshot_day: recovery.day(),
                observation_day: recording.day(),
            });
        }
        let mut staged_budget = self.public_rows;
        let day_update =
            staged_budget.reconcile(recording.day(), recovery.plugin_hook_public_rows())?;

        Ok(PluginHookAggregateStage {
            destination_budget: &mut self.public_rows,
            staged_budget,
            entries: StagedEntries::new(&mut self.entries, day_update == OpenDayUpdate::Advanced),
            recovery,
            recording,
        })
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

/// Copy-on-write plugin-hook admission for one recording operation.
///
/// Each selection's borrow ends before the next selection begins. The caller
/// must derive invocation-wide facts from its observations rather than from
/// several simultaneously borrowed private entries.
#[must_use = "dropping a plugin-hook aggregate stage rolls back its private-state edits"]
pub(in crate::telemetry) struct PluginHookAggregateStage<'store, 'context, 'identity> {
    destination_budget: &'store mut DailyPublicRowBudget,
    staged_budget: DailyPublicRowBudget,
    entries: StagedEntries<'store, PluginHookMetricsKey, PluginHookAggregateState>,
    recovery: &'context PublicAggregateRecoveryIndex,
    recording: &'context BoundRecordingObservation<'identity>,
}

impl PluginHookAggregateStage<'_, '_, '_> {
    /// Select or admit the aggregate for one plugin-hook terminal result.
    ///
    /// Adoption happens before overflow and does not spend another public-row
    /// slot. Repeated selections reuse the same staged entry.
    pub(in crate::telemetry) fn select(
        &mut self,
        agent: HookAgent,
        surface: HookSurface,
        attribution: PluginHookAttribution,
    ) -> Result<SelectedPluginHookAggregate<'_>, PluginHookAdmissionError> {
        let bucket = AdmittedPluginBucket::from_attribution(self.recording, attribution);
        let key = PluginHookMetricsKey::new(self.recording, agent, surface, &bucket);

        if self.entries.contains_key(&key) {
            return Self::select_existing(&mut self.entries, &key);
        }

        if let Some((plugin, subject)) = bucket.public_identity() {
            if let Some(event_id) = self
                .recovery
                .plugin_hook_event_id(agent, surface, plugin, subject)
            {
                return Self::insert_recovered(&mut self.entries, key, bucket, event_id);
            }

            if self.staged_budget.is_exhausted() {
                return Self::select_public_at_capacity(&mut self.entries, key);
            }

            let PublicRowAdmission::Public = self.staged_budget.admit_new() else {
                unreachable!("BUG: a non-exhausted public-row budget must admit a row");
            };
        }

        Self::insert_new(&mut self.entries, key, bucket)
    }

    fn select_public_at_capacity<'a>(
        entries: &'a mut StagedEntries<'_, PluginHookMetricsKey, PluginHookAggregateState>,
        mut key: PluginHookMetricsKey,
    ) -> Result<SelectedPluginHookAggregate<'a>, PluginHookAdmissionError> {
        let bucket = AdmittedPluginBucket::overflow();
        key.bucket = bucket.key();
        Self::select_or_insert(entries, key, bucket)
    }

    fn select_or_insert<'a>(
        entries: &'a mut StagedEntries<'_, PluginHookMetricsKey, PluginHookAggregateState>,
        key: PluginHookMetricsKey,
        bucket: AdmittedPluginBucket,
    ) -> Result<SelectedPluginHookAggregate<'a>, PluginHookAdmissionError> {
        let state = entries.get_or_insert_with(key.clone(), |key| {
            PluginHookAggregateState::new(key, bucket)
        });
        state
            .select(&key)
            .map_err(PluginHookAdmissionError::PrivateState)
    }

    fn select_existing<'a>(
        entries: &'a mut StagedEntries<'_, PluginHookMetricsKey, PluginHookAggregateState>,
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

    fn insert_new<'a>(
        entries: &'a mut StagedEntries<'_, PluginHookMetricsKey, PluginHookAggregateState>,
        key: PluginHookMetricsKey,
        bucket: AdmittedPluginBucket,
    ) -> Result<SelectedPluginHookAggregate<'a>, PluginHookAdmissionError> {
        let state = entries.get_or_insert_with(key.clone(), |key| {
            PluginHookAggregateState::new(key, bucket)
        });
        state
            .select(&key)
            .map_err(PluginHookAdmissionError::PrivateState)
    }

    fn insert_recovered<'a>(
        entries: &'a mut StagedEntries<'_, PluginHookMetricsKey, PluginHookAggregateState>,
        key: PluginHookMetricsKey,
        bucket: AdmittedPluginBucket,
        event_id: crate::telemetry::schema::EventId,
    ) -> Result<SelectedPluginHookAggregate<'a>, PluginHookAdmissionError> {
        let state = entries.get_or_insert_with(key.clone(), |key| {
            PluginHookAggregateState::recovered(key, bucket, event_id)
        });
        state
            .select(&key)
            .map_err(PluginHookAdmissionError::PrivateState)
    }

    /// Apply the staged allowance and entries without a recoverable failure.
    ///
    /// Hook-invocation recording commits this alongside the hook and
    /// extension-invocation stages. This method must remain infallible so that
    /// sequence cannot stop after committing only part of the private state.
    pub(in crate::telemetry) fn commit(self) {
        let Self {
            destination_budget,
            staged_budget,
            entries,
            recovery: _,
            recording: _,
        } = self;
        *destination_budget = staged_budget;
        entries.commit();
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
