//! Disjoint private stores for cumulative telemetry rows.

use super::{
    extension_invocation::ExtensionInvocationAggregateStore, hook::HookAggregateStore,
    plugin_hook::PluginHookAggregateStore,
};
use crate::telemetry::schema::UtcDay;

/// Private state owned by the three aggregate row families.
///
/// One Claude skill-hook observation can update hook, plugin-hook, and
/// extension-invocation rows. Keeping their stores as separate fields and
/// returning all three borrows together gives the coordinator the complete
/// private-state atomic unit without borrowing a parent object repeatedly.
/// Once that coordinator lands, it becomes the only code allowed to create
/// and commit per-store stages; dropping the coordinator rolls all of them
/// back together.
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
        extension_invocation.clear();

        hook_stage.commit();
        plugin_hook_stage.commit();
    }
}
