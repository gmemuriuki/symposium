//! Disjoint private stores for cumulative telemetry rows.

use super::{
    extension_invocation::ExtensionInvocationAggregateStore, hook::HookAggregateStore,
    plugin_hook::PluginHookAggregateStore,
};
use crate::telemetry::schema::UtcDay;

/// Result of consuming one aggregate private-state stage.
///
/// Committing remains infallible because one recording operation applies
/// several stores sequentially. A stage poisoned by failed selection instead
/// reports that it discarded every staged edit.
#[must_use = "aggregate stage commit outcomes must be observed"]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::telemetry) enum StageCommit {
    /// Every staged edit was applied to its destination store.
    Applied,
    /// A selection failed, so every staged edit was discarded.
    DiscardedPoisoned,
}

/// Private state owned by the three aggregate row families.
///
/// One Claude skill-hook observation can update hook, plugin-hook, and
/// extension-invocation rows. Keeping their stores as separate fields and
/// returning all three borrows together gives the coordinator the complete
/// private-state atomic unit without borrowing a parent object repeatedly.
/// That coordinator creates all three stages on every recording, including
/// operations with no aggregate observations, so their days advance together.
/// It becomes the only code allowed to commit them: success applies all three,
/// while any staging or row-update failure drops all three. Row-update failure
/// is safe because touched trackers live in the overlays and each row mutates
/// its tracker only after its other checked updates succeed.
///
/// When these stores join the persisted schema, decoding must validate each
/// store day against the identity high-water day in `state/codec.rs`. A
/// disagreement is invalid state rather than a day to reconcile silently.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::telemetry) struct AggregateState {
    hook: HookAggregateStore,
    plugin_hook: PluginHookAggregateStore,
    extension_invocation: ExtensionInvocationAggregateStore,
}

impl AggregateState {
    #[must_use]
    pub(in crate::telemetry) const fn new(day: UtcDay) -> Self {
        Self {
            hook: HookAggregateStore::new(day),
            plugin_hook: PluginHookAggregateStore::new(day),
            extension_invocation: ExtensionInvocationAggregateStore::new(day),
        }
    }

    /// Borrow every store that one top-level hook invocation can change.
    #[must_use]
    pub(in crate::telemetry) const fn hook_invocation_stores(
        &mut self,
    ) -> (
        &mut HookAggregateStore,
        &mut PluginHookAggregateStore,
        &mut ExtensionInvocationAggregateStore,
    ) {
        (
            &mut self.hook,
            &mut self.plugin_hook,
            &mut self.extension_invocation,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telemetry::{
        state::{IDENTIFIER_WINDOW_TEST_STATE, TelemetryStateV1, recording_observation},
        storage::metrics::MetricSnapshot,
    };

    #[test]
    fn hook_invocation_stages_can_remain_live_together() {
        let mut state: TelemetryStateV1 = toml::from_str(IDENTIFIER_WINDOW_TEST_STATE).unwrap();
        let recording = recording_observation(&mut state);
        let recovery = MetricSnapshot::empty(recording.day()).public_recovery_index();
        let mut aggregates = AggregateState::new(recording.day());

        let (hook, plugin_hook, extension_invocation) = aggregates.hook_invocation_stores();
        let hook_stage = hook.stage(&recording).unwrap();
        let plugin_hook_stage = plugin_hook.stage(&recovery, &recording).unwrap();
        let extension_invocation_stage = extension_invocation.stage(&recovery, &recording).unwrap();

        assert_eq!(hook_stage.commit(), StageCommit::Applied);
        assert_eq!(plugin_hook_stage.commit(), StageCommit::Applied);
        assert_eq!(extension_invocation_stage.commit(), StageCommit::Applied);
    }
}
