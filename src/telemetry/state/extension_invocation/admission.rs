//! Bounded admission and private selection for invocation aggregates.

use std::{
    collections::{
        BTreeMap,
        btree_map::{Entry, OccupiedEntry, VacantEntry},
    },
    fmt,
};

use super::ExtensionSessionCountTracker;
use crate::telemetry::{
    identity::{AgentSubject, ExtensionSubject},
    schema::{
        EventId, ExtensionInvocationAgent, ExtensionInvocationAttribution, ExtensionTargetScope,
        PublicSkillCoordinate, SupportedAgent, UnnamedExtensionReason, UtcDay,
    },
    state::{
        BoundRecordingObservation, DayBeforeCurrent,
        open_day::OpenDayUpdate,
        public_row_budget::{DailyPublicRowBudget, PublicRowAdmission},
    },
    storage::metrics::PublicAggregateRecoveryIndex,
};

/// Identity fields admitted for one extension-invocation aggregate row.
///
/// The inner enum is private so only state admission can create an overflow
/// bucket or pair a public target with its scoped subject.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::telemetry) struct AdmittedExtensionBucket(AdmittedExtensionBucketKind);

#[derive(Debug, Clone, PartialEq, Eq)]
enum AdmittedExtensionBucketKind {
    Public {
        target: PublicSkillCoordinate,
        subject: ExtensionSubject,
    },
    Unnamed(UnnamedExtensionReason),
    Overflow,
}

impl AdmittedExtensionBucket {
    fn from_attribution(
        recording: &BoundRecordingObservation<'_>,
        attribution: ExtensionInvocationAttribution,
    ) -> Self {
        match attribution {
            ExtensionInvocationAttribution::Public(attribution) => {
                let subject = attribution.derive_subject(recording.identifier_window_scope());
                let target = attribution.target().clone();
                Self(AdmittedExtensionBucketKind::Public { target, subject })
            }
            ExtensionInvocationAttribution::Unnamed(reason) => {
                Self(AdmittedExtensionBucketKind::Unnamed(reason))
            }
        }
    }

    const fn overflow() -> Self {
        Self(AdmittedExtensionBucketKind::Overflow)
    }

    #[must_use]
    pub(in crate::telemetry) const fn scope(&self) -> ExtensionTargetScope {
        match self.0 {
            AdmittedExtensionBucketKind::Public { .. } => ExtensionTargetScope::Public,
            AdmittedExtensionBucketKind::Unnamed(_) => ExtensionTargetScope::Unnamed,
            AdmittedExtensionBucketKind::Overflow => ExtensionTargetScope::Overflow,
        }
    }

    #[must_use]
    pub(in crate::telemetry) const fn target(&self) -> Option<&PublicSkillCoordinate> {
        match &self.0 {
            AdmittedExtensionBucketKind::Public { target, .. } => Some(target),
            AdmittedExtensionBucketKind::Unnamed(_) | AdmittedExtensionBucketKind::Overflow => None,
        }
    }

    #[must_use]
    pub(in crate::telemetry) const fn unnamed_reason(&self) -> Option<UnnamedExtensionReason> {
        match self.0 {
            AdmittedExtensionBucketKind::Unnamed(reason) => Some(reason),
            AdmittedExtensionBucketKind::Public { .. } | AdmittedExtensionBucketKind::Overflow => {
                None
            }
        }
    }

    #[must_use]
    pub(in crate::telemetry) const fn extension_subject(&self) -> Option<ExtensionSubject> {
        match self.0 {
            AdmittedExtensionBucketKind::Public { subject, .. } => Some(subject),
            AdmittedExtensionBucketKind::Unnamed(_) | AdmittedExtensionBucketKind::Overflow => None,
        }
    }

    /// Return the target and derived subject of a public bucket together.
    #[must_use]
    const fn public_identity(&self) -> Option<(&PublicSkillCoordinate, ExtensionSubject)> {
        match &self.0 {
            AdmittedExtensionBucketKind::Public { target, subject } => Some((target, *subject)),
            AdmittedExtensionBucketKind::Unnamed(_) | AdmittedExtensionBucketKind::Overflow => None,
        }
    }

    const fn key(&self) -> ExtensionInvocationBucketKey {
        match self.0 {
            AdmittedExtensionBucketKind::Public { subject, .. } => {
                ExtensionInvocationBucketKey::Public(subject)
            }
            AdmittedExtensionBucketKind::Unnamed(reason) => {
                ExtensionInvocationBucketKey::Unnamed(reason)
            }
            AdmittedExtensionBucketKind::Overflow => ExtensionInvocationBucketKey::Overflow,
        }
    }
}

/// Private lookup key for one daily invocation aggregate.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(in crate::telemetry) struct ExtensionInvocationAggregateKey {
    day: UtcDay,
    // The row needs the plaintext agent; its subject separates private epochs.
    agent: ExtensionInvocationAgent,
    agent_subject: AgentSubject,
    bucket: ExtensionInvocationBucketKey,
}

impl ExtensionInvocationAggregateKey {
    fn new(
        recording: &BoundRecordingObservation<'_>,
        agent: ExtensionInvocationAgent,
        bucket: &AdmittedExtensionBucket,
    ) -> Self {
        let agent_subject = recording
            .identifier_window_scope()
            .derive(&SupportedAgent::from(agent));

        Self {
            day: recording.day(),
            agent,
            agent_subject,
            bucket: bucket.key(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum ExtensionInvocationBucketKey {
    Public(ExtensionSubject),
    Unnamed(UnnamedExtensionReason),
    Overflow,
}

/// Private state paired with one extension-invocation aggregate row.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ExtensionInvocationAggregateState {
    event_id: EventId,
    bucket: AdmittedExtensionBucket,
    session_counts: ExtensionSessionCountTracker<ExtensionInvocationAggregateKey>,
}

impl ExtensionInvocationAggregateState {
    fn new(key: &ExtensionInvocationAggregateKey, bucket: AdmittedExtensionBucket) -> Self {
        Self::with_event_id(key, bucket, EventId::new())
    }

    /// Recover private state for a surviving public snapshot row.
    ///
    /// The fresh tracker deliberately disagrees with a non-empty row's
    /// contribution baseline. Its first later update therefore marks both
    /// session sets incomplete while aggregate totals continue from the row.
    fn recovered(
        key: &ExtensionInvocationAggregateKey,
        bucket: AdmittedExtensionBucket,
        event_id: EventId,
    ) -> Self {
        Self::with_event_id(key, bucket, event_id)
    }

    fn with_event_id(
        key: &ExtensionInvocationAggregateKey,
        bucket: AdmittedExtensionBucket,
        event_id: EventId,
    ) -> Self {
        Self {
            event_id,
            bucket,
            session_counts: ExtensionSessionCountTracker::new(key.clone()),
        }
    }

    fn select(
        &mut self,
        key: &ExtensionInvocationAggregateKey,
    ) -> Result<SelectedExtensionInvocationAggregate<'_>, ExtensionAggregateSelectionError> {
        if self.session_counts.key() != key {
            return Err(ExtensionAggregateSelectionError);
        }

        self.select_admitted()
    }

    fn select_admitted(
        &mut self,
    ) -> Result<SelectedExtensionInvocationAggregate<'_>, ExtensionAggregateSelectionError> {
        if self.bucket.key() != self.session_counts.key().bucket {
            return Err(ExtensionAggregateSelectionError);
        }

        Ok(SelectedExtensionInvocationAggregate {
            event_id: self.event_id,
            bucket: &self.bucket,
            session_counts: &mut self.session_counts,
        })
    }
}

/// Key-checked access to one admitted extension-invocation aggregate.
pub(in crate::telemetry) struct SelectedExtensionInvocationAggregate<'a> {
    event_id: EventId,
    bucket: &'a AdmittedExtensionBucket,
    session_counts: &'a mut ExtensionSessionCountTracker<ExtensionInvocationAggregateKey>,
}

impl SelectedExtensionInvocationAggregate<'_> {
    #[must_use]
    pub(in crate::telemetry) const fn event_id(&self) -> EventId {
        self.event_id
    }

    #[must_use]
    pub(in crate::telemetry) const fn day(&self) -> UtcDay {
        self.session_counts.key().day
    }

    #[must_use]
    pub(in crate::telemetry) const fn agent(&self) -> ExtensionInvocationAgent {
        self.session_counts.key().agent
    }

    #[must_use]
    pub(in crate::telemetry) const fn bucket(&self) -> &AdmittedExtensionBucket {
        self.bucket
    }

    /// Return whether this selection belongs to the supplied recording
    /// context, including its identifier epoch.
    #[must_use]
    pub(in crate::telemetry) fn matches_recording(
        &self,
        recording: &BoundRecordingObservation<'_>,
    ) -> bool {
        let key = self.session_counts.key();
        key.day == recording.day()
            && key.agent_subject
                == recording
                    .identifier_window_scope()
                    .derive(&SupportedAgent::from(key.agent))
    }

    #[must_use]
    pub(in crate::telemetry) const fn session_counts(
        &mut self,
    ) -> &mut ExtensionSessionCountTracker<ExtensionInvocationAggregateKey> {
        self.session_counts
    }
}

/// Daily private state for extension-invocation aggregate admission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::telemetry) struct ExtensionInvocationAggregateStore {
    public_rows: DailyPublicRowBudget,
    entries: BTreeMap<ExtensionInvocationAggregateKey, ExtensionInvocationAggregateState>,
}

impl ExtensionInvocationAggregateStore {
    #[must_use]
    pub(in crate::telemetry) const fn new(day: UtcDay) -> Self {
        Self {
            public_rows: DailyPublicRowBudget::new(day),
            entries: BTreeMap::new(),
        }
    }

    /// Select or admit the aggregate for one attributed observation.
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
        agent: ExtensionInvocationAgent,
        attribution: ExtensionInvocationAttribution,
    ) -> Result<SelectedExtensionInvocationAggregate<'_>, ExtensionInvocationAdmissionError> {
        if recovery.day() != recording.day() {
            return Err(ExtensionInvocationAdmissionError::SnapshotDayMismatch {
                snapshot_day: recovery.day(),
                observation_day: recording.day(),
            });
        }
        if self
            .public_rows
            .reconcile(recording.day(), recovery.extension_invocation_public_rows())?
            == OpenDayUpdate::Advanced
        {
            self.entries.clear();
        }

        let bucket = AdmittedExtensionBucket::from_attribution(recording, attribution);
        let key = ExtensionInvocationAggregateKey::new(recording, agent, &bucket);
        // Returning a selection keeps the mutable map borrow alive. Check
        // first, then perform the borrowing lookup in a separate helper so
        // the absent path can still insert on stable Rust.
        if self.entries.contains_key(&key) {
            return Self::select_existing(&mut self.entries, &key);
        }

        if let Some((target, subject)) = bucket.public_identity() {
            if let Some(event_id) = recovery.extension_invocation_event_id(agent, target, subject) {
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
        entries: &mut BTreeMap<ExtensionInvocationAggregateKey, ExtensionInvocationAggregateState>,
        mut key: ExtensionInvocationAggregateKey,
    ) -> Result<SelectedExtensionInvocationAggregate<'_>, ExtensionInvocationAdmissionError> {
        let bucket = AdmittedExtensionBucket::overflow();
        key.bucket = bucket.key();
        Self::select_or_insert(entries, key, bucket)
    }

    fn select_or_insert(
        entries: &mut BTreeMap<ExtensionInvocationAggregateKey, ExtensionInvocationAggregateState>,
        key: ExtensionInvocationAggregateKey,
        bucket: AdmittedExtensionBucket,
    ) -> Result<SelectedExtensionInvocationAggregate<'_>, ExtensionInvocationAdmissionError> {
        match entries.entry(key) {
            Entry::Occupied(entry) => Self::select_occupied(entry),
            Entry::Vacant(entry) => Self::insert_vacant(entry, bucket),
        }
    }

    fn select_existing<'a>(
        entries: &'a mut BTreeMap<
            ExtensionInvocationAggregateKey,
            ExtensionInvocationAggregateState,
        >,
        key: &ExtensionInvocationAggregateKey,
    ) -> Result<SelectedExtensionInvocationAggregate<'a>, ExtensionInvocationAdmissionError> {
        let Some(entry) = entries.get_mut(key) else {
            return Err(ExtensionInvocationAdmissionError::PrivateState(
                ExtensionAggregateSelectionError,
            ));
        };
        entry
            .select(key)
            .map_err(ExtensionInvocationAdmissionError::PrivateState)
    }

    fn insert_new(
        entries: &mut BTreeMap<ExtensionInvocationAggregateKey, ExtensionInvocationAggregateState>,
        key: ExtensionInvocationAggregateKey,
        bucket: AdmittedExtensionBucket,
    ) -> Result<SelectedExtensionInvocationAggregate<'_>, ExtensionInvocationAdmissionError> {
        let Entry::Vacant(entry) = entries.entry(key) else {
            return Err(ExtensionInvocationAdmissionError::PrivateState(
                ExtensionAggregateSelectionError,
            ));
        };
        Self::insert_vacant(entry, bucket)
    }

    fn insert_recovered(
        entries: &mut BTreeMap<ExtensionInvocationAggregateKey, ExtensionInvocationAggregateState>,
        key: ExtensionInvocationAggregateKey,
        bucket: AdmittedExtensionBucket,
        event_id: EventId,
    ) -> Result<SelectedExtensionInvocationAggregate<'_>, ExtensionInvocationAdmissionError> {
        let Entry::Vacant(entry) = entries.entry(key) else {
            return Err(ExtensionInvocationAdmissionError::PrivateState(
                ExtensionAggregateSelectionError,
            ));
        };
        let key = entry.key().clone();
        entry
            .insert(ExtensionInvocationAggregateState::recovered(
                &key, bucket, event_id,
            ))
            .select(&key)
            .map_err(ExtensionInvocationAdmissionError::PrivateState)
    }

    fn select_occupied(
        entry: OccupiedEntry<
            '_,
            ExtensionInvocationAggregateKey,
            ExtensionInvocationAggregateState,
        >,
    ) -> Result<SelectedExtensionInvocationAggregate<'_>, ExtensionInvocationAdmissionError> {
        if entry.get().session_counts.key() != entry.key() {
            return Err(ExtensionInvocationAdmissionError::PrivateState(
                ExtensionAggregateSelectionError,
            ));
        }

        entry
            .into_mut()
            .select_admitted()
            .map_err(ExtensionInvocationAdmissionError::PrivateState)
    }

    fn insert_vacant(
        entry: VacantEntry<'_, ExtensionInvocationAggregateKey, ExtensionInvocationAggregateState>,
        bucket: AdmittedExtensionBucket,
    ) -> Result<SelectedExtensionInvocationAggregate<'_>, ExtensionInvocationAdmissionError> {
        let state = ExtensionInvocationAggregateState::new(entry.key(), bucket);
        entry
            .insert(state)
            .select_admitted()
            .map_err(ExtensionInvocationAdmissionError::PrivateState)
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

/// Failure while selecting private extension-invocation aggregate state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::telemetry) enum ExtensionInvocationAdmissionError {
    SnapshotDayMismatch {
        snapshot_day: UtcDay,
        observation_day: UtcDay,
    },
    DayBeforeCurrent(DayBeforeCurrent),
    PrivateState(ExtensionAggregateSelectionError),
}

impl From<DayBeforeCurrent> for ExtensionInvocationAdmissionError {
    fn from(error: DayBeforeCurrent) -> Self {
        Self::DayBeforeCurrent(error)
    }
}

impl fmt::Display for ExtensionInvocationAdmissionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SnapshotDayMismatch {
                snapshot_day,
                observation_day,
            } => write!(
                formatter,
                "extension-invocation snapshot belongs to {snapshot_day}, not observation day {observation_day}"
            ),
            Self::DayBeforeCurrent(error) => {
                write!(formatter, "extension-invocation budget {error}")
            }
            Self::PrivateState(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for ExtensionInvocationAdmissionError {}

/// A private entry selected with another aggregate key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::telemetry) struct ExtensionAggregateSelectionError;

impl fmt::Display for ExtensionAggregateSelectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("extension-invocation private state belongs to another aggregate")
    }
}

impl std::error::Error for ExtensionAggregateSelectionError {}

#[cfg(test)]
mod tests;
