use std::time::Duration;

use chrono::{TimeZone as _, Utc};

use super::*;
use crate::telemetry::{
    schema::{
        AggregateRow, ExtensionInvocationMetricsUpdateError, ExtensionInvocationPhase, HookOutcome,
        PluginHookOutcomeSignals, RowClassification, TelemetryRow, UnnamedExtensionReason,
        UtcSecond, classify_row,
    },
    state::{IDENTIFIER_WINDOW_TEST_STATE, TelemetryStateV1, recording_observation},
};

fn state() -> TelemetryStateV1 {
    toml::from_str(IDENTIFIER_WINDOW_TEST_STATE).unwrap()
}

fn public_plugin(name: &str) -> PluginHookAttribution {
    let coordinate = serde_json::from_value(serde_json::json!({
        "source": "symposium-recommendations",
        "name": name,
    }))
    .unwrap();
    PluginHookAttribution::Public(coordinate)
}

fn terminal_plugin(name: &str) -> PluginHookInvocationObservation {
    PluginHookInvocationObservation {
        attribution: public_plugin(name),
        terminal: Some(PluginHookAttempt::Executed {
            signals: PluginHookOutcomeSignals {
                error: false,
                blocked: false,
            },
            prepare_duration: Duration::from_millis(2),
            execute_duration: Duration::from_millis(3),
        }),
    }
}

fn non_terminal_plugin(name: &str) -> PluginHookInvocationObservation {
    PluginHookInvocationObservation {
        attribution: public_plugin(name),
        terminal: None,
    }
}

fn extension(phase: ExtensionInvocationPhase) -> ExtensionInvocationObservation {
    ExtensionInvocationObservation {
        attribution: ExtensionInvocationAttribution::Unnamed(
            UnnamedExtensionReason::AttributionUnavailable,
        ),
        phase,
    }
}

fn invocation(
    plugins: Vec<PluginHookInvocationObservation>,
    extension: Option<ExtensionInvocationObservation>,
) -> HookInvocationMetricObservation<'static> {
    HookInvocationMetricObservation {
        agent: HookAgent::Claude,
        hook: HookSurface::PreToolUse,
        outcome: HookOutcome::Ok,
        duration: Duration::from_millis(12),
        vendor_session_id: None,
        plugin_attempts: plugins,
        extension,
    }
}

fn row_values(staged: &StagedAggregateRecording) -> Vec<serde_json::Value> {
    staged
        .snapshot
        .rows()
        .map(|row| serde_json::to_value(row).unwrap())
        .collect()
}

#[test]
fn non_terminal_plugin_contributes_only_to_top_level_attempts() {
    let mut identity = state();
    let recording = recording_observation(&mut identity);
    let mut aggregates = AggregateState::new(recording.day());
    let snapshot = MetricSnapshot::empty(recording.day());

    let staged = aggregates
        .stage_hook_invocation(
            &recording,
            snapshot,
            invocation(
                vec![
                    terminal_plugin("plugin-a"),
                    non_terminal_plugin("plugin-b"),
                    terminal_plugin("plugin-c"),
                ],
                None,
            ),
        )
        .unwrap();
    let rows = row_values(&staged);
    let hook = rows
        .iter()
        .find(|row| row["kind"] == "hook_metrics")
        .unwrap();
    let plugin_rows: Vec<_> = rows
        .iter()
        .filter(|row| row["kind"] == "plugin_hook_metrics")
        .collect();

    assert_eq!(hook["plugins_attempted"], 3);
    assert_eq!(hook["plugins_completed"], 2);
    assert_eq!(plugin_rows.len(), 2);
    assert!(plugin_rows.iter().all(|row| row["attempts"] == 1));
}

#[test]
fn extension_failure_discards_hook_plugin_and_extension_private_state() {
    let mut identity = state();
    let recording = recording_observation(&mut identity);
    let mut aggregates = AggregateState::new(recording.day());
    let initial = aggregates
        .stage_hook_invocation(
            &recording,
            MetricSnapshot::empty(recording.day()),
            invocation(
                Vec::new(),
                Some(extension(ExtensionInvocationPhase::Attempted)),
            ),
        )
        .unwrap();
    let mut snapshot = initial.snapshot;
    let extension_id = snapshot
        .rows()
        .find_map(|row| match row {
            AggregateRow::ExtensionInvocation(_) => Some(row.event_id()),
            AggregateRow::Hook(_) | AggregateRow::PluginHook(_) => None,
        })
        .unwrap();
    let extension_row = snapshot.row(extension_id).unwrap();
    let mut value = serde_json::to_value(extension_row).unwrap();
    value["attempted"] = serde_json::Value::from(u64::MAX);
    let line = serde_json::to_string(&value).unwrap();
    let RowClassification::Supported(TelemetryRow::Aggregate(row)) = classify_row(&line) else {
        panic!("modified extension row must remain valid");
    };
    snapshot.replace(row).unwrap();
    let aggregates_before = aggregates.clone();

    let result = aggregates.stage_hook_invocation(
        &recording,
        snapshot,
        invocation(
            vec![terminal_plugin("plugin-a")],
            Some(extension(ExtensionInvocationPhase::Attempted)),
        ),
    );

    assert!(matches!(
        result,
        Err(AggregateRecordingError::ExtensionRow(
            ExtensionInvocationMetricsUpdateError::PhaseCountOverflow {
                phase: ExtensionInvocationPhase::Attempted,
            }
        ))
    ));
    assert_eq!(aggregates, aggregates_before);
}

#[test]
fn snapshot_day_mismatch_discards_every_private_stage() {
    let mut identity = state();
    let recording = recording_observation(&mut identity);
    let mut aggregates = AggregateState::new(recording.day());
    let aggregates_before = aggregates.clone();
    let tomorrow =
        UtcSecond::from_datetime(Utc.with_ymd_and_hms(2026, 8, 4, 0, 0, 0).unwrap()).day();

    let result = aggregates.stage_hook_invocation(
        &recording,
        MetricSnapshot::empty(tomorrow),
        invocation(Vec::new(), None),
    );

    assert!(matches!(
        result,
        Err(AggregateRecordingError::SnapshotDayMismatch {
            snapshot_day,
            recording_day,
        }) if snapshot_day == tomorrow && recording_day == recording.day()
    ));
    assert_eq!(aggregates, aggregates_before);
}

#[test]
fn extension_phase_must_match_the_top_level_hook_surface() {
    let mut identity = state();
    let recording = recording_observation(&mut identity);
    let mut aggregates = AggregateState::new(recording.day());
    let before = aggregates.clone();

    let result = aggregates.stage_hook_invocation(
        &recording,
        MetricSnapshot::empty(recording.day()),
        invocation(
            Vec::new(),
            Some(extension(ExtensionInvocationPhase::Completed)),
        ),
    );

    assert!(matches!(
        result,
        Err(AggregateRecordingError::ExtensionPhaseDoesNotMatchHook {
            hook: HookSurface::PreToolUse,
            phase: ExtensionInvocationPhase::Completed,
        })
    ));
    assert_eq!(aggregates, before);
}

#[test]
fn successful_invocation_advances_all_three_families() {
    let mut identity = state();
    let recording = recording_observation(&mut identity);
    let mut aggregates = AggregateState::new(recording.day());
    let before = aggregates.clone();

    let staged = aggregates
        .stage_hook_invocation(
            &recording,
            MetricSnapshot::empty(recording.day()),
            invocation(
                vec![terminal_plugin("plugin-a")],
                Some(extension(ExtensionInvocationPhase::Attempted)),
            ),
        )
        .unwrap();
    let rows = row_values(&staged);

    assert_ne!(aggregates.hook, before.hook);
    assert_ne!(aggregates.plugin_hook, before.plugin_hook);
    assert_ne!(aggregates.extension_invocation, before.extension_invocation);
    assert_eq!(rows.len(), 3);
    assert!(rows.iter().any(|row| row["kind"] == "hook_metrics"));
    assert!(rows.iter().any(|row| row["kind"] == "plugin_hook_metrics"));
    assert!(
        rows.iter()
            .any(|row| row["kind"] == "extension_invocation_metrics")
    );
}

#[test]
fn a_later_invocation_recovers_and_updates_the_existing_hook_row() {
    let mut identity = state();
    let recording = recording_observation(&mut identity);
    let mut aggregates = AggregateState::new(recording.day());
    let first = aggregates
        .stage_hook_invocation(
            &recording,
            MetricSnapshot::empty(recording.day()),
            invocation(Vec::new(), None),
        )
        .unwrap();

    let second = aggregates
        .stage_hook_invocation(&recording, first.snapshot, invocation(Vec::new(), None))
        .unwrap();
    let hook_rows: Vec<_> = row_values(&second)
        .into_iter()
        .filter(|row| row["kind"] == "hook_metrics")
        .collect();

    assert_eq!(hook_rows.len(), 1);
    assert_eq!(hook_rows[0]["invocations"], 2);
}

#[test]
fn hook_only_rollover_advances_all_three_store_days() {
    let mut identity = state();
    let completed_at =
        UtcSecond::from_datetime(Utc.with_ymd_and_hms(2026, 8, 4, 10, 2, 11).unwrap());
    let observed = identity.observe_recording(completed_at).unwrap();
    let recording = identity.bind_recording_observation(observed).unwrap();
    let previous_day =
        UtcSecond::from_datetime(Utc.with_ymd_and_hms(2026, 8, 3, 10, 2, 11).unwrap()).day();
    let mut aggregates = AggregateState::new(previous_day);
    let before = aggregates.clone();

    let staged = aggregates
        .stage_hook_invocation(
            &recording,
            MetricSnapshot::empty(recording.day()),
            invocation(Vec::new(), None),
        )
        .unwrap();
    drop(staged);

    assert_eq!(aggregates.hook.day(), recording.day());
    assert_ne!(aggregates.plugin_hook, before.plugin_hook);
    assert_ne!(aggregates.extension_invocation, before.extension_invocation);
}

#[test]
fn repeated_plugin_bucket_updates_one_row() {
    let mut identity = state();
    let recording = recording_observation(&mut identity);
    let mut aggregates = AggregateState::new(recording.day());

    let staged = aggregates
        .stage_hook_invocation(
            &recording,
            MetricSnapshot::empty(recording.day()),
            invocation(
                vec![terminal_plugin("plugin-a"), terminal_plugin("plugin-a")],
                None,
            ),
        )
        .unwrap();
    let rows = row_values(&staged);
    let plugin_rows: Vec<_> = rows
        .iter()
        .filter(|row| row["kind"] == "plugin_hook_metrics")
        .collect();

    assert_eq!(plugin_rows.len(), 1);
    assert_eq!(plugin_rows[0]["attempts"], 2);
}
