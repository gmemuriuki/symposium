//! Vocabulary for resolution telemetry.

use serde::{Deserialize, Serialize};

use super::DroppedOperation;

/// Operation that caused a full resolution and sync.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::telemetry) enum ResolutionTrigger {
    SessionStart,
    ManualSync,
    Use,
    Remove,
}

impl From<ResolutionTrigger> for DroppedOperation {
    fn from(trigger: ResolutionTrigger) -> Self {
        match trigger {
            ResolutionTrigger::SessionStart => Self::SessionStart,
            ResolutionTrigger::ManualSync => Self::ManualSync,
            ResolutionTrigger::Use => Self::Use,
            ResolutionTrigger::Remove => Self::Remove,
        }
    }
}

/// Result of a completed full resolution and sync.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::telemetry) enum ResolutionOutcome {
    Ok,
    Partial,
    Error,
}

/// One reason that a package coordinate cannot be named.
///
/// The public-identity policy selects this reason after applying source
/// provenance precedence. Recording accepts one selected reason at a time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(in crate::telemetry) enum UnnamedPackageReason {
    PrivateRegistry,
    Git,
    Path,
    Workspace,
    UnknownSource,
    InvalidCoordinate,
}

/// Mutually exclusive reasons that package coordinates cannot be named.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::telemetry) struct UnnamedPackageReasons {
    private_registry: u64,
    git: u64,
    path: u64,
    workspace: u64,
    unknown_source: u64,
    invalid_coordinate: u64,
}

impl UnnamedPackageReasons {
    /// Increment exactly one reason counter, or return `None` on overflow.
    #[must_use = "counter overflow must drop the containing telemetry batch"]
    pub(in crate::telemetry) fn checked_record(
        &mut self,
        reason: UnnamedPackageReason,
    ) -> Option<()> {
        let counter = match reason {
            UnnamedPackageReason::PrivateRegistry => &mut self.private_registry,
            UnnamedPackageReason::Git => &mut self.git,
            UnnamedPackageReason::Path => &mut self.path,
            UnnamedPackageReason::Workspace => &mut self.workspace,
            UnnamedPackageReason::UnknownSource => &mut self.unknown_source,
            UnnamedPackageReason::InvalidCoordinate => &mut self.invalid_coordinate,
        };

        *counter = counter.checked_add(1)?;
        Some(())
    }

    /// Return the total number of unnamed packages, or `None` on overflow.
    #[must_use]
    fn checked_total(self) -> Option<u64> {
        [
            self.private_registry,
            self.git,
            self.path,
            self.workspace,
            self.unknown_source,
            self.invalid_coordinate,
        ]
        .into_iter()
        .try_fold(0_u64, u64::checked_add)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn example_reasons() -> UnnamedPackageReasons {
        UnnamedPackageReasons {
            private_registry: 1,
            git: 2,
            path: 3,
            workspace: 4,
            unknown_source: 5,
            invalid_coordinate: 6,
        }
    }

    #[test]
    fn resolution_triggers_round_trip_and_match_storage_names() {
        let cases = [
            (ResolutionTrigger::SessionStart, "session_start"),
            (ResolutionTrigger::ManualSync, "manual_sync"),
            (ResolutionTrigger::Use, "use"),
            (ResolutionTrigger::Remove, "remove"),
        ];

        for (trigger, name) in cases {
            let json = serde_json::to_string(&trigger).unwrap();
            let decoded = serde_json::from_str::<ResolutionTrigger>(&json).unwrap();
            let dropped_operation =
                serde_json::to_string(&DroppedOperation::from(trigger)).unwrap();

            assert_eq!(json, format!(r#""{name}""#));
            assert_eq!(decoded, trigger);
            assert_eq!(dropped_operation, json);
        }
    }

    #[test]
    fn resolution_outcomes_round_trip_with_contract_names() {
        let cases = [
            (ResolutionOutcome::Ok, "ok"),
            (ResolutionOutcome::Partial, "partial"),
            (ResolutionOutcome::Error, "error"),
        ];

        for (outcome, name) in cases {
            let json = serde_json::to_string(&outcome).unwrap();
            let decoded = serde_json::from_str::<ResolutionOutcome>(&json).unwrap();

            assert_eq!(json, format!(r#""{name}""#));
            assert_eq!(decoded, outcome);
        }
    }

    #[test]
    fn resolution_vocabulary_rejects_unknown_contract_names() {
        let unknown = r#""future_value""#;

        let trigger = serde_json::from_str::<ResolutionTrigger>(unknown);
        let outcome = serde_json::from_str::<ResolutionOutcome>(unknown);

        assert!(trigger.is_err());
        assert!(outcome.is_err());
    }

    #[test]
    fn unnamed_package_reasons_round_trip_in_contract_order() {
        let reasons = example_reasons();

        let json = serde_json::to_string(&reasons).unwrap();
        let decoded = serde_json::from_str::<UnnamedPackageReasons>(&json).unwrap();

        assert_eq!(
            json,
            r#"{"private_registry":1,"git":2,"path":3,"workspace":4,"unknown_source":5,"invalid_coordinate":6}"#
        );
        assert_eq!(decoded, reasons);
    }

    #[test]
    fn unnamed_package_reasons_reject_unknown_fields() {
        let json = r#"{"private_registry":1,"git":2,"path":3,"workspace":4,"unknown_source":5,"invalid_coordinate":6,"future_source":7}"#;

        let result = serde_json::from_str::<UnnamedPackageReasons>(json);

        assert!(result.is_err());
    }

    #[test]
    fn unnamed_package_reasons_require_every_contract_field() {
        let json = r#"{"private_registry":1,"git":2,"path":3,"workspace":4,"unknown_source":5}"#;

        let result = serde_json::from_str::<UnnamedPackageReasons>(json);

        assert!(result.is_err());
    }

    #[test]
    fn unnamed_package_reason_total_uses_checked_arithmetic() {
        let reasons = example_reasons();

        let total = reasons.checked_total();

        assert_eq!(total, Some(21));
    }

    #[test]
    fn recording_a_reason_increments_only_its_counter() {
        let cases = [
            (
                UnnamedPackageReason::PrivateRegistry,
                UnnamedPackageReasons {
                    private_registry: 1,
                    ..UnnamedPackageReasons::default()
                },
            ),
            (
                UnnamedPackageReason::Git,
                UnnamedPackageReasons {
                    git: 1,
                    ..UnnamedPackageReasons::default()
                },
            ),
            (
                UnnamedPackageReason::Path,
                UnnamedPackageReasons {
                    path: 1,
                    ..UnnamedPackageReasons::default()
                },
            ),
            (
                UnnamedPackageReason::Workspace,
                UnnamedPackageReasons {
                    workspace: 1,
                    ..UnnamedPackageReasons::default()
                },
            ),
            (
                UnnamedPackageReason::UnknownSource,
                UnnamedPackageReasons {
                    unknown_source: 1,
                    ..UnnamedPackageReasons::default()
                },
            ),
            (
                UnnamedPackageReason::InvalidCoordinate,
                UnnamedPackageReasons {
                    invalid_coordinate: 1,
                    ..UnnamedPackageReasons::default()
                },
            ),
        ];

        for (reason, expected) in cases {
            let mut reasons = UnnamedPackageReasons::default();

            let recorded = reasons.checked_record(reason);

            assert_eq!(recorded, Some(()));
            assert_eq!(reasons, expected);
        }
    }

    #[test]
    fn recording_a_reason_rejects_overflow_without_mutation() {
        let mut reasons = UnnamedPackageReasons {
            private_registry: u64::MAX,
            ..UnnamedPackageReasons::default()
        };
        let before = reasons;

        let recorded = reasons.checked_record(UnnamedPackageReason::PrivateRegistry);

        assert_eq!(recorded, None);
        assert_eq!(reasons, before);
    }

    #[test]
    fn unnamed_package_reason_total_rejects_overflow() {
        let reasons = UnnamedPackageReasons {
            private_registry: u64::MAX,
            git: 1,
            ..UnnamedPackageReasons::default()
        };

        let total = reasons.checked_total();

        assert_eq!(total, None);
    }
}
