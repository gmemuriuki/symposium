//! Closed vocabulary shared by agent-originated telemetry rows.

use serde::{Deserialize, Serialize};

/// Agent that invoked a registered Symposium hook.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum HookAgent {
    Claude,
    Codex,
    Copilot,
    Gemini,
    Kiro,
}

/// Operating-system class for the running Symposium build.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum OperatingSystem {
    Linux,
    Macos,
    Windows,
    Other,
}

/// Architecture class for the running Symposium build.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Architecture {
    X86_64,
    Aarch64,
    Other,
}

/// Agent-supplied classification of how a session began.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum SessionStartKind {
    Fresh,
    Resumed,
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hook_agents_round_trip_with_contract_names() {
        let cases = [
            (HookAgent::Claude, "claude"),
            (HookAgent::Codex, "codex"),
            (HookAgent::Copilot, "copilot"),
            (HookAgent::Gemini, "gemini"),
            (HookAgent::Kiro, "kiro"),
        ];

        for (agent, name) in cases {
            let json = serde_json::to_string(&agent).unwrap();
            let decoded = serde_json::from_str::<HookAgent>(&json).unwrap();

            assert_eq!(json, format!(r#""{name}""#));
            assert_eq!(decoded, agent);
        }
    }

    #[test]
    fn operating_systems_round_trip_with_contract_names() {
        let cases = [
            (OperatingSystem::Linux, "linux"),
            (OperatingSystem::Macos, "macos"),
            (OperatingSystem::Windows, "windows"),
            (OperatingSystem::Other, "other"),
        ];

        for (operating_system, name) in cases {
            let json = serde_json::to_string(&operating_system).unwrap();
            let decoded = serde_json::from_str::<OperatingSystem>(&json).unwrap();

            assert_eq!(json, format!(r#""{name}""#));
            assert_eq!(decoded, operating_system);
        }
    }

    #[test]
    fn architectures_round_trip_with_contract_names() {
        let cases = [
            (Architecture::X86_64, "x86_64"),
            (Architecture::Aarch64, "aarch64"),
            (Architecture::Other, "other"),
        ];

        for (architecture, name) in cases {
            let json = serde_json::to_string(&architecture).unwrap();
            let decoded = serde_json::from_str::<Architecture>(&json).unwrap();

            assert_eq!(json, format!(r#""{name}""#));
            assert_eq!(decoded, architecture);
        }
    }

    #[test]
    fn session_start_kinds_round_trip_with_contract_names() {
        let cases = [
            (SessionStartKind::Fresh, "fresh"),
            (SessionStartKind::Resumed, "resumed"),
            (SessionStartKind::Unknown, "unknown"),
        ];

        for (start_kind, name) in cases {
            let json = serde_json::to_string(&start_kind).unwrap();
            let decoded = serde_json::from_str::<SessionStartKind>(&json).unwrap();

            assert_eq!(json, format!(r#""{name}""#));
            assert_eq!(decoded, start_kind);
        }
    }

    #[test]
    fn agent_vocabulary_rejects_unknown_contract_names() {
        let unknown = r#""future_value""#;

        let hook_agent = serde_json::from_str::<HookAgent>(unknown);
        let operating_system = serde_json::from_str::<OperatingSystem>(unknown);
        let architecture = serde_json::from_str::<Architecture>(unknown);
        let start_kind = serde_json::from_str::<SessionStartKind>(unknown);

        assert!(hook_agent.is_err());
        assert!(operating_system.is_err());
        assert!(architecture.is_err());
        assert!(start_kind.is_err());
    }
}
