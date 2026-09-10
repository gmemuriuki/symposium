//! Closed vocabulary shared by agent-originated telemetry rows.

use serde::{Deserialize, Serialize};

use super::{
    CohortDay, EventId, RowKind, SchemaVersion, SymposiumVersion, UtcDay, UtcSecond,
    deserialize_version_one,
};
use crate::{
    agents::Agent,
    telemetry::identity::{AgentSubject, RetentionSubject, SessionId},
};

/// Agent included in the daily configuration snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::telemetry) enum SupportedAgent {
    Claude,
    Codex,
    Copilot,
    Gemini,
    Kiro,
    #[serde(rename = "opencode")]
    OpenCode,
    Goose,
}

impl From<Agent> for SupportedAgent {
    fn from(agent: Agent) -> Self {
        match agent {
            Agent::Claude => Self::Claude,
            Agent::Codex => Self::Codex,
            Agent::Copilot => Self::Copilot,
            Agent::Gemini => Self::Gemini,
            Agent::Kiro => Self::Kiro,
            Agent::OpenCode => Self::OpenCode,
            Agent::Goose => Self::Goose,
        }
    }
}

/// Agent that invoked a registered Symposium hook.
///
/// Unlike the platform enums, this has no `Other`: Symposium owns the set of
/// registered agent hooks, so adding an agent changes the row schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::telemetry) enum HookAgent {
    Claude,
    Codex,
    Copilot,
    Gemini,
    Kiro,
}

/// Operating-system class for the running Symposium build.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::telemetry) enum OperatingSystem {
    Linux,
    Macos,
    Windows,
    Other,
}

impl OperatingSystem {
    /// Return the contract bucket for the running Symposium build.
    #[must_use]
    pub(in crate::telemetry) fn current() -> Self {
        Self::from_target(std::env::consts::OS)
    }

    fn from_target(target: &str) -> Self {
        match target {
            "linux" => Self::Linux,
            "macos" => Self::Macos,
            "windows" => Self::Windows,
            _ => Self::Other,
        }
    }
}

/// Architecture class for the running Symposium build.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::telemetry) enum Architecture {
    X86_64,
    Aarch64,
    Other,
}

impl Architecture {
    /// Return the contract bucket for the running Symposium build.
    #[must_use]
    pub(in crate::telemetry) fn current() -> Self {
        Self::from_target(std::env::consts::ARCH)
    }

    fn from_target(target: &str) -> Self {
        match target {
            "x86_64" => Self::X86_64,
            "aarch64" => Self::Aarch64,
            _ => Self::Other,
        }
    }
}

/// Agent-supplied classification of how a session began.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::telemetry) enum SessionStartKind {
    Fresh,
    Resumed,
    Unknown,
}

/// Agent-supplied and derived fields for one completed session-start hook.
///
/// These fields are repeated on [`SessionStartV1`] because flattening this
/// struct into the row would weaken strict unknown-field rejection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::telemetry) struct SessionStartFields {
    pub(in crate::telemetry) agent: HookAgent,
    pub(in crate::telemetry) os: OperatingSystem,
    pub(in crate::telemetry) arch: Architecture,
    pub(in crate::telemetry) start: SessionStartKind,
    pub(in crate::telemetry) session_id: Option<SessionId>,
    pub(in crate::telemetry) retention_subject: RetentionSubject,
    pub(in crate::telemetry) cohort_day: CohortDay,
}

/// Version 1 record of a completed registered session-start hook.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::telemetry) struct SessionStartV1 {
    #[serde(rename = "v", deserialize_with = "deserialize_version_one")]
    version: SchemaVersion,
    kind: RowKind,
    event_id: EventId,
    day: UtcDay,
    at: UtcSecond,
    symposium: SymposiumVersion,
    agent: HookAgent,
    os: OperatingSystem,
    arch: Architecture,
    start: SessionStartKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    session_id: Option<SessionId>,
    retention_subject: RetentionSubject,
    cohort_day: CohortDay,
}

impl SessionStartV1 {
    /// Create a record for a completed registered session-start hook.
    #[must_use]
    pub(in crate::telemetry) fn new(at: UtcSecond, fields: SessionStartFields) -> Self {
        Self {
            version: SchemaVersion::V1,
            kind: RowKind::SessionStart,
            event_id: EventId::new(),
            day: at.day(),
            at,
            symposium: SymposiumVersion::current(),
            agent: fields.agent,
            os: fields.os,
            arch: fields.arch,
            start: fields.start,
            session_id: fields.session_id,
            retention_subject: fields.retention_subject,
            cohort_day: fields.cohort_day,
        }
    }
}

/// Version 1 daily observation of one supported agent's configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::telemetry) struct AgentConfigurationV1 {
    #[serde(rename = "v", deserialize_with = "deserialize_version_one")]
    version: SchemaVersion,
    kind: RowKind,
    event_id: EventId,
    day: UtcDay,
    symposium: SymposiumVersion,
    agent: SupportedAgent,
    configured: bool,
    os: OperatingSystem,
    arch: Architecture,
    agent_subject: AgentSubject,
}

impl AgentConfigurationV1 {
    /// Create one agent entry in a daily configuration snapshot.
    #[must_use]
    pub(in crate::telemetry) fn new(
        day: UtcDay,
        agent: SupportedAgent,
        configured: bool,
        os: OperatingSystem,
        arch: Architecture,
        agent_subject: AgentSubject,
    ) -> Self {
        Self {
            version: SchemaVersion::V1,
            kind: RowKind::AgentConfiguration,
            event_id: EventId::new(),
            day,
            symposium: SymposiumVersion::current(),
            agent,
            configured,
            os,
            arch,
            agent_subject,
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::{NaiveDate, TimeZone, Utc};

    use super::super::{RowClassification, TelemetryRow, classify_row};
    use super::*;

    fn session_start_fields(session_id: Option<SessionId>) -> SessionStartFields {
        SessionStartFields {
            agent: HookAgent::Claude,
            os: OperatingSystem::Linux,
            arch: Architecture::X86_64,
            start: SessionStartKind::Fresh,
            session_id,
            retention_subject: "ret_74ddf26f80ad8b58de7f03e6c632e654".parse().unwrap(),
            cohort_day: CohortDay::D0,
        }
    }

    fn session_start_time() -> UtcSecond {
        UtcSecond::from_datetime(Utc.with_ymd_and_hms(2026, 8, 3, 9, 14, 2).unwrap())
    }

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
    fn supported_agents_round_trip_with_contract_names() {
        let cases = [
            (SupportedAgent::Claude, "claude"),
            (SupportedAgent::Codex, "codex"),
            (SupportedAgent::Copilot, "copilot"),
            (SupportedAgent::Gemini, "gemini"),
            (SupportedAgent::Kiro, "kiro"),
            (SupportedAgent::OpenCode, "opencode"),
            (SupportedAgent::Goose, "goose"),
        ];

        for (agent, name) in cases {
            let json = serde_json::to_string(&agent).unwrap();
            let decoded = serde_json::from_str::<SupportedAgent>(&json).unwrap();

            assert_eq!(json, format!(r#""{name}""#));
            assert_eq!(decoded, agent);
        }
    }

    #[test]
    fn project_agents_convert_to_supported_telemetry_agents() {
        let cases = [
            (Agent::Claude, SupportedAgent::Claude),
            (Agent::Codex, SupportedAgent::Codex),
            (Agent::Copilot, SupportedAgent::Copilot),
            (Agent::Gemini, SupportedAgent::Gemini),
            (Agent::Kiro, SupportedAgent::Kiro),
            (Agent::OpenCode, SupportedAgent::OpenCode),
            (Agent::Goose, SupportedAgent::Goose),
        ];

        for (agent, expected) in cases {
            assert_eq!(SupportedAgent::from(agent), expected);
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
    fn operating_system_target_names_map_to_contract_buckets() {
        let cases = [
            ("linux", OperatingSystem::Linux),
            ("macos", OperatingSystem::Macos),
            ("windows", OperatingSystem::Windows),
            ("freebsd", OperatingSystem::Other),
        ];

        for (target, expected) in cases {
            assert_eq!(OperatingSystem::from_target(target), expected);
        }
    }

    #[test]
    fn current_operating_system_matches_the_compile_target() {
        #[cfg(target_os = "linux")]
        let expected = OperatingSystem::Linux;
        #[cfg(target_os = "macos")]
        let expected = OperatingSystem::Macos;
        #[cfg(target_os = "windows")]
        let expected = OperatingSystem::Windows;
        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        let expected = OperatingSystem::Other;

        assert_eq!(OperatingSystem::current(), expected);
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
    fn architecture_target_names_map_to_contract_buckets() {
        let cases = [
            ("x86_64", Architecture::X86_64),
            ("aarch64", Architecture::Aarch64),
            ("riscv64", Architecture::Other),
        ];

        for (target, expected) in cases {
            assert_eq!(Architecture::from_target(target), expected);
        }
    }

    #[test]
    fn current_architecture_matches_the_compile_target() {
        #[cfg(target_arch = "x86_64")]
        let expected = Architecture::X86_64;
        #[cfg(target_arch = "aarch64")]
        let expected = Architecture::Aarch64;
        #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
        let expected = Architecture::Other;

        assert_eq!(Architecture::current(), expected);
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

        let supported_agent = serde_json::from_str::<SupportedAgent>(unknown);
        let hook_agent = serde_json::from_str::<HookAgent>(unknown);
        let operating_system = serde_json::from_str::<OperatingSystem>(unknown);
        let architecture = serde_json::from_str::<Architecture>(unknown);
        let start_kind = serde_json::from_str::<SessionStartKind>(unknown);

        assert!(supported_agent.is_err());
        assert!(hook_agent.is_err());
        assert!(operating_system.is_err());
        assert!(architecture.is_err());
        assert!(start_kind.is_err());
    }

    #[test]
    fn new_session_start_uses_fixed_common_fields_and_timestamp_day() {
        let at = session_start_time();
        let session_id = "sess_31d8b1916028f65a0c0521dc1f4c86fb".parse().unwrap();
        let fields = session_start_fields(Some(session_id));

        let row = SessionStartV1::new(at, fields);

        assert_eq!(row.version, SchemaVersion::V1);
        assert_eq!(row.kind, RowKind::SessionStart);
        assert_eq!(row.event_id.0.get_version(), Some(uuid::Version::Random));
        assert_eq!(row.day, at.day());
        assert_eq!(row.at, at);
        assert_eq!(row.symposium, SymposiumVersion::current());
        assert_eq!(row.agent, fields.agent);
        assert_eq!(row.os, fields.os);
        assert_eq!(row.arch, fields.arch);
        assert_eq!(row.start, fields.start);
        assert_eq!(row.session_id, fields.session_id);
        assert_eq!(row.retention_subject, fields.retention_subject);
        assert_eq!(row.cohort_day, fields.cohort_day);
    }

    #[test]
    fn new_agent_configuration_uses_fixed_common_fields() {
        let day = UtcDay::from_date(NaiveDate::from_ymd_opt(2026, 8, 3).unwrap());
        let agent_subject = "agt_9255770e1679cb789796a9f9e86325c5".parse().unwrap();

        let row = AgentConfigurationV1::new(
            day,
            SupportedAgent::Claude,
            true,
            OperatingSystem::Linux,
            Architecture::X86_64,
            agent_subject,
        );

        assert_eq!(row.version, SchemaVersion::V1);
        assert_eq!(row.kind, RowKind::AgentConfiguration);
        assert_eq!(row.event_id.0.get_version(), Some(uuid::Version::Random));
        assert_eq!(row.day, day);
        assert_eq!(row.symposium, SymposiumVersion::current());
        assert_eq!(row.agent, SupportedAgent::Claude);
        assert!(row.configured);
        assert_eq!(row.os, OperatingSystem::Linux);
        assert_eq!(row.arch, Architecture::X86_64);
        assert_eq!(row.agent_subject, agent_subject);
    }

    #[test]
    fn session_start_without_session_id_classifies_and_round_trips() {
        let row = SessionStartV1::new(session_start_time(), session_start_fields(None));

        let json = serde_json::to_string(&row).unwrap();
        let value = serde_json::from_str::<serde_json::Value>(&json).unwrap();
        let RowClassification::Supported(TelemetryRow::SessionStart(decoded)) = classify_row(&json)
        else {
            panic!("session_start without a session id was not classified as supported");
        };

        assert_eq!(value.get("session_id"), None);
        assert!(decoded.session_id.is_none());
        assert_eq!(serde_json::to_string(&decoded).unwrap(), json);
    }
}
