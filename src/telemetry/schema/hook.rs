//! Hook aggregate vocabulary and identity dimensions.

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

impl std::fmt::Display for UnsupportedHookEvent {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
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
