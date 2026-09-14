//! Hook aggregate vocabulary, counters, and identity dimensions.

use std::fmt;

use serde::{Deserialize, Serialize};

use super::agent::HookAgent;
use crate::{
    hook_schema::HookEvent,
    telemetry::identity::{DimensionWriter, HookDomain, IdentityDimension},
};

/// Hook surface included in version 1 aggregate telemetry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::telemetry) enum HookSurface {
    PreToolUse,
    PostToolUse,
    UserPromptSubmit,
    SessionStart,
    Stop,
}

impl HookSurface {
    /// Return the frozen version 1 wire label.
    #[must_use]
    const fn as_str(self) -> &'static str {
        match self {
            Self::PreToolUse => "pre_tool_use",
            Self::PostToolUse => "post_tool_use",
            Self::UserPromptSubmit => "user_prompt_submit",
            Self::SessionStart => "session_start",
            Self::Stop => "stop",
        }
    }
}

/// A hook event that version 1 telemetry does not aggregate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::telemetry) struct UnsupportedHookEvent;

impl fmt::Display for UnsupportedHookEvent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("hook event is not supported by telemetry consent version 1")
    }
}

impl std::error::Error for UnsupportedHookEvent {}

impl TryFrom<HookEvent> for HookSurface {
    type Error = UnsupportedHookEvent;

    fn try_from(event: HookEvent) -> Result<Self, Self::Error> {
        match event {
            HookEvent::PreToolUse => Ok(Self::PreToolUse),
            HookEvent::PostToolUse => Ok(Self::PostToolUse),
            HookEvent::UserPromptSubmit => Ok(Self::UserPromptSubmit),
            HookEvent::SessionStart => Ok(Self::SessionStart),
            HookEvent::Stop => Ok(Self::Stop),
            // HookEvent is non-exhaustive. New SDK events need an explicit
            // consent-contract decision before telemetry records them.
            _ => Err(UnsupportedHookEvent),
        }
    }
}

/// Final outcome assigned to one completed top-level hook observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::telemetry) enum HookOutcome {
    Ok,
    Blocked,
    PluginError,
    InternalError,
}

impl HookOutcome {
    /// Select the final outcome using the version 1 precedence rule.
    #[must_use]
    pub(in crate::telemetry) const fn from_signals(signals: HookOutcomeSignals) -> Self {
        if signals.internal_error {
            Self::InternalError
        } else if signals.blocked {
            Self::Blocked
        } else if signals.plugin_error {
            Self::PluginError
        } else {
            Self::Ok
        }
    }

    /// Return the counter name associated with this outcome.
    #[must_use]
    const fn counter_name(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Blocked => "blocked",
            Self::PluginError => "plugin_error",
            Self::InternalError => "internal_error",
        }
    }
}

/// Signals used to select one final top-level hook outcome.
///
/// Named fields prevent the three precedence inputs from being transposed at
/// call sites.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::telemetry) struct HookOutcomeSignals {
    pub(in crate::telemetry) internal_error: bool,
    pub(in crate::telemetry) blocked: bool,
    pub(in crate::telemetry) plugin_error: bool,
}

/// Mutually exclusive outcomes of completed top-level hook observations.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HookOutcomeCounters {
    ok: u64,
    blocked: u64,
    plugin_error: u64,
    internal_error: u64,
}

impl HookOutcomeCounters {
    /// Increment exactly one outcome counter.
    ///
    /// # Errors
    ///
    /// Returns [`HookOutcomeCounterOverflow`] when the selected counter cannot
    /// be incremented. The counters are unchanged on failure.
    #[must_use = "counter overflow must drop the containing telemetry update"]
    fn checked_record(&mut self, outcome: HookOutcome) -> Result<(), HookOutcomeCounterOverflow> {
        let counter = match outcome {
            HookOutcome::Ok => &mut self.ok,
            HookOutcome::Blocked => &mut self.blocked,
            HookOutcome::PluginError => &mut self.plugin_error,
            HookOutcome::InternalError => &mut self.internal_error,
        };
        let next = counter
            .checked_add(1)
            .ok_or(HookOutcomeCounterOverflow { outcome })?;

        *counter = next;
        Ok(())
    }

    /// Return the sum of every outcome counter, or `None` on overflow.
    #[must_use]
    fn checked_total(&self) -> Option<u64> {
        [
            self.ok,
            self.blocked,
            self.plugin_error,
            self.internal_error,
        ]
        .into_iter()
        .try_fold(0_u64, u64::checked_add)
    }
}

/// A selected hook outcome counter that cannot be incremented.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HookOutcomeCounterOverflow {
    outcome: HookOutcome,
}

impl fmt::Display for HookOutcomeCounterOverflow {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} hook outcome counter overflows u64",
            self.outcome.counter_name()
        )
    }
}

impl std::error::Error for HookOutcomeCounterOverflow {}

/// Typed inputs to one `hook_subject` derivation.
struct HookDimension {
    agent: HookAgent,
    hook: HookSurface,
}

impl HookDimension {
    #[must_use]
    const fn new(agent: HookAgent, hook: HookSurface) -> Self {
        Self { agent, hook }
    }
}

impl IdentityDimension for HookDimension {
    type Domain = HookDomain;

    /// Write agent and hook surface in version 1 contract order.
    fn write(&self, writer: &mut DimensionWriter<'_>) {
        writer.field(self.agent.as_str().as_bytes());
        writer.field(self.hook.as_str().as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::super::{
        IDENTIFIER_WINDOW_TEST_STATE, assert_contract_names_with_labels, recording_observation,
    };
    use super::*;
    use crate::telemetry::{
        identity::{HookSubject, encode_dimension_for_test},
        state::TelemetryStateV1,
    };

    fn hook_subject(agent: HookAgent, hook: HookSurface) -> HookSubject {
        let mut state: TelemetryStateV1 = toml::from_str(IDENTIFIER_WINDOW_TEST_STATE).unwrap();
        let observation = recording_observation(&mut state);
        let dimension = HookDimension::new(agent, hook);

        observation.identifier_window_scope().derive(&dimension)
    }

    #[test]
    fn hook_surfaces_round_trip_with_contract_names() {
        let cases = [
            (HookSurface::PreToolUse, "pre_tool_use"),
            (HookSurface::PostToolUse, "post_tool_use"),
            (HookSurface::UserPromptSubmit, "user_prompt_submit"),
            (HookSurface::SessionStart, "session_start"),
            (HookSurface::Stop, "stop"),
        ];

        assert_contract_names_with_labels(&cases, HookSurface::as_str);
    }

    #[test]
    fn sdk_hook_events_map_to_contract_surfaces() {
        let cases = [
            (HookEvent::PreToolUse, HookSurface::PreToolUse),
            (HookEvent::PostToolUse, HookSurface::PostToolUse),
            (HookEvent::UserPromptSubmit, HookSurface::UserPromptSubmit),
            (HookEvent::SessionStart, HookSurface::SessionStart),
            (HookEvent::Stop, HookSurface::Stop),
        ];

        for (event, expected) in cases {
            assert_eq!(HookSurface::try_from(event), Ok(expected));
        }
    }

    #[test]
    fn hook_outcome_counters_round_trip_in_contract_order() {
        let counters = HookOutcomeCounters {
            ok: 1,
            blocked: 2,
            plugin_error: 3,
            internal_error: 4,
        };
        let expected = r#"{"ok":1,"blocked":2,"plugin_error":3,"internal_error":4}"#;

        let json = serde_json::to_string(&counters).unwrap();
        let decoded = serde_json::from_str::<HookOutcomeCounters>(&json).unwrap();

        assert_eq!(json, expected);
        assert_eq!(decoded, counters);
    }

    #[test]
    fn recording_each_hook_outcome_increments_only_its_counter() {
        let cases = [
            (
                HookOutcome::Ok,
                r#"{"ok":1,"blocked":0,"plugin_error":0,"internal_error":0}"#,
            ),
            (
                HookOutcome::Blocked,
                r#"{"ok":0,"blocked":1,"plugin_error":0,"internal_error":0}"#,
            ),
            (
                HookOutcome::PluginError,
                r#"{"ok":0,"blocked":0,"plugin_error":1,"internal_error":0}"#,
            ),
            (
                HookOutcome::InternalError,
                r#"{"ok":0,"blocked":0,"plugin_error":0,"internal_error":1}"#,
            ),
        ];

        for (outcome, expected) in cases {
            let mut counters = HookOutcomeCounters::default();

            counters.checked_record(outcome).unwrap();

            let json = serde_json::to_string(&counters).unwrap();
            let value = serde_json::to_value(counters).unwrap();
            let active_counter = value
                .as_object()
                .unwrap()
                .iter()
                .find_map(|(name, count)| (count.as_u64() == Some(1)).then_some(name.as_str()))
                .unwrap();

            assert_eq!(json, expected);
            assert_eq!(active_counter, outcome.counter_name());
        }
    }

    #[test]
    fn hook_outcome_uses_the_contract_precedence() {
        let cases = [
            (
                HookOutcomeSignals {
                    internal_error: false,
                    blocked: false,
                    plugin_error: false,
                },
                HookOutcome::Ok,
            ),
            (
                HookOutcomeSignals {
                    internal_error: false,
                    blocked: false,
                    plugin_error: true,
                },
                HookOutcome::PluginError,
            ),
            (
                HookOutcomeSignals {
                    internal_error: false,
                    blocked: true,
                    plugin_error: true,
                },
                HookOutcome::Blocked,
            ),
            (
                HookOutcomeSignals {
                    internal_error: true,
                    blocked: true,
                    plugin_error: true,
                },
                HookOutcome::InternalError,
            ),
        ];

        for (signals, expected) in cases {
            assert_eq!(HookOutcome::from_signals(signals), expected);
        }
    }

    #[test]
    fn hook_outcome_counters_reject_missing_unknown_and_invalid_fields() {
        let cases = [
            r#"{"ok":0,"blocked":0,"plugin_error":0}"#,
            r#"{"ok":0,"blocked":0,"plugin_error":0,"internal_error":0,"future":0}"#,
            r#"{"ok":"none","blocked":0,"plugin_error":0,"internal_error":0}"#,
            r#"{"ok":-1,"blocked":0,"plugin_error":0,"internal_error":0}"#,
        ];

        for json in cases {
            assert!(
                serde_json::from_str::<HookOutcomeCounters>(json).is_err(),
                "accepted invalid hook outcome counters {json}"
            );
        }
    }

    #[test]
    fn recording_an_outcome_rejects_overflow_without_mutation() {
        let outcomes = [
            HookOutcome::Ok,
            HookOutcome::Blocked,
            HookOutcome::PluginError,
            HookOutcome::InternalError,
        ];

        for outcome in outcomes {
            let mut counters = HookOutcomeCounters {
                ok: u64::MAX,
                blocked: u64::MAX,
                plugin_error: u64::MAX,
                internal_error: u64::MAX,
            };
            let before = counters;

            let result = counters.checked_record(outcome);

            assert_eq!(result, Err(HookOutcomeCounterOverflow { outcome }));
            assert_eq!(counters, before);
        }
    }

    #[test]
    fn hook_outcome_counter_total_is_checked_for_overflow() {
        let representable = HookOutcomeCounters {
            ok: 1,
            blocked: 2,
            plugin_error: 3,
            internal_error: 4,
        };
        let overflowing = HookOutcomeCounters {
            ok: u64::MAX,
            blocked: 1,
            plugin_error: 0,
            internal_error: 0,
        };

        assert_eq!(representable.checked_total(), Some(10));
        assert_eq!(overflowing.checked_total(), None);
    }

    #[test]
    fn hook_surface_rejects_unknown_contract_names() {
        let result = serde_json::from_str::<HookSurface>(r#""before_tool_use""#);

        assert!(result.is_err());
    }

    #[test]
    fn hook_subject_dimension_uses_agent_then_hook_surface() {
        let dimension = HookDimension::new(HookAgent::Claude, HookSurface::PreToolUse);
        let expected = [
            [0, 0, 0, 0, 0, 0, 0, 6].as_slice(),
            b"claude".as_slice(),
            [0, 0, 0, 0, 0, 0, 0, 12].as_slice(),
            b"pre_tool_use".as_slice(),
        ]
        .concat();

        let encoded = encode_dimension_for_test(&dimension);

        assert_eq!(encoded, expected);
    }

    #[test]
    fn hook_subject_derivation_matches_independent_vector() {
        // Cross-checked with .NET's HMACSHA256 over the contract header,
        // identifier window, agent, and hook surface. The complete digest is
        // 99106b457209912cf0023c562a72dafb225707f0718f26e3a379c42fedf1cf55.
        let expected = "hok_99106b457209912cf0023c562a72dafb".parse().unwrap();

        let subject = hook_subject(HookAgent::Claude, HookSurface::PreToolUse);

        assert_eq!(subject, expected);
    }

    #[test]
    fn hook_subject_changes_with_agent_or_hook_surface() {
        let baseline = hook_subject(HookAgent::Claude, HookSurface::PreToolUse);
        let other_agent = hook_subject(HookAgent::Codex, HookSurface::PreToolUse);
        let other_surface = hook_subject(HookAgent::Claude, HookSurface::PostToolUse);

        assert_ne!(baseline, other_agent);
        assert_ne!(baseline, other_surface);
    }
}
