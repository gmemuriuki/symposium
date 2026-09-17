//! Plugin-hook aggregate vocabulary and public identity.

use std::fmt;

use serde::{Deserialize, Serialize};

use super::extension::{
    ExtensionKind, PublicExtensionCoordinate, PublicExtensionName, PublicExtensionNameError,
    PublicExtensionSource,
};
use crate::telemetry::identity::{DimensionWriter, IdentityDimension, PluginDomain};

/// Identity exposure assigned to one plugin-hook aggregate bucket.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::telemetry) enum PluginScope {
    Public,
    Unnamed,
    Overflow,
}

/// Final outcome assigned to one completed plugin-hook attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::telemetry) enum PluginHookOutcome {
    Ok,
    Blocked,
    Error,
}

impl PluginHookOutcome {
    /// Select the final outcome using the version 1 precedence rule.
    #[must_use]
    pub(in crate::telemetry) const fn from_signals(signals: PluginHookOutcomeSignals) -> Self {
        if signals.error {
            Self::Error
        } else if signals.blocked {
            Self::Blocked
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
            Self::Error => "error",
        }
    }
}

/// Signals used to select one final plugin-hook outcome.
///
/// Named fields prevent the precedence inputs from being transposed at call
/// sites.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::telemetry) struct PluginHookOutcomeSignals {
    pub(in crate::telemetry) error: bool,
    pub(in crate::telemetry) blocked: bool,
}

/// Mutually exclusive outcomes of completed plugin-hook attempts.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PluginHookOutcomeCounters {
    ok: u64,
    blocked: u64,
    error: u64,
}

impl PluginHookOutcomeCounters {
    /// Increment exactly one outcome counter.
    ///
    /// # Errors
    ///
    /// Returns [`PluginHookOutcomeCounterOverflow`] when the selected counter
    /// cannot be incremented. The counters are unchanged on failure.
    #[must_use = "counter overflow must drop the containing telemetry update"]
    fn checked_record(
        &mut self,
        outcome: PluginHookOutcome,
    ) -> Result<(), PluginHookOutcomeCounterOverflow> {
        let counter = match outcome {
            PluginHookOutcome::Ok => &mut self.ok,
            PluginHookOutcome::Blocked => &mut self.blocked,
            PluginHookOutcome::Error => &mut self.error,
        };
        let next = counter
            .checked_add(1)
            .ok_or(PluginHookOutcomeCounterOverflow { outcome })?;

        *counter = next;
        Ok(())
    }

    /// Return the sum of every outcome counter, or `None` on overflow.
    #[must_use]
    fn checked_total(&self) -> Option<u64> {
        [self.ok, self.blocked, self.error]
            .into_iter()
            .try_fold(0_u64, u64::checked_add)
    }
}

/// A selected plugin-hook outcome counter that cannot be incremented.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PluginHookOutcomeCounterOverflow {
    outcome: PluginHookOutcome,
}

impl fmt::Display for PluginHookOutcomeCounterOverflow {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} plugin-hook outcome counter overflows u64",
            self.outcome.counter_name()
        )
    }
}

impl std::error::Error for PluginHookOutcomeCounterOverflow {}

/// Public plugin coordinate safe to place in plugin-hook telemetry.
///
/// The enclosing row establishes that this coordinate identifies a plugin,
/// so its wire form contains only the reviewed public source and validated
/// name.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::telemetry) struct PublicPluginCoordinate {
    source: PublicExtensionSource,
    name: PublicExtensionName,
}

impl PublicPluginCoordinate {
    /// Combine validated components into one public plugin coordinate.
    #[must_use]
    pub(in crate::telemetry) const fn new(
        source: PublicExtensionSource,
        name: PublicExtensionName,
    ) -> Self {
        Self { source, name }
    }

    /// Validate a raw public name and combine it with an allowlisted source.
    ///
    /// # Errors
    ///
    /// Returns an error when `name` is outside the version 1 public extension
    /// grammar.
    pub(in crate::telemetry) fn try_new(
        source: PublicExtensionSource,
        name: &str,
    ) -> Result<Self, PublicExtensionNameError> {
        Ok(Self::new(source, name.parse()?))
    }

    /// Return the reviewed public source.
    #[must_use]
    pub(in crate::telemetry) const fn source(&self) -> PublicExtensionSource {
        self.source
    }

    /// Return the validated public plugin name.
    #[must_use]
    pub(in crate::telemetry) const fn name(&self) -> &PublicExtensionName {
        &self.name
    }
}

impl TryFrom<&PublicExtensionCoordinate> for PublicPluginCoordinate {
    type Error = NotPublicPlugin;

    fn try_from(coordinate: &PublicExtensionCoordinate) -> Result<Self, Self::Error> {
        if coordinate.kind() != ExtensionKind::Plugin {
            return Err(NotPublicPlugin {
                found: coordinate.kind(),
            });
        }

        Ok(Self::new(coordinate.source(), coordinate.name().clone()))
    }
}

impl IdentityDimension for PublicPluginCoordinate {
    type Domain = PluginDomain;

    /// Write public source and plugin name in version 1 contract order.
    fn write(&self, writer: &mut DimensionWriter<'_>) {
        writer.field(self.source.as_str().as_bytes());
        writer.field(self.name.as_str().as_bytes());
    }
}

/// A public extension coordinate that identifies something other than a plugin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::telemetry) struct NotPublicPlugin {
    found: ExtensionKind,
}

impl fmt::Display for NotPublicPlugin {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "expected a public plugin coordinate, found {}",
            self.found.as_str()
        )
    }
}

impl std::error::Error for NotPublicPlugin {}

#[cfg(test)]
mod tests {
    use super::super::{
        IDENTIFIER_WINDOW_TEST_STATE, assert_contract_names, recording_observation,
    };
    use super::*;
    use crate::telemetry::{
        identity::{PluginSubject, encode_dimension_for_test},
        state::TelemetryStateV1,
    };

    fn public_extension(
        kind: ExtensionKind,
        source: PublicExtensionSource,
        name: &str,
    ) -> PublicExtensionCoordinate {
        PublicExtensionCoordinate::try_new(kind, source, name).unwrap()
    }

    fn public_plugin(source: PublicExtensionSource, name: &str) -> PublicPluginCoordinate {
        PublicPluginCoordinate::try_new(source, name).unwrap()
    }

    fn plugin_subject(coordinate: &PublicPluginCoordinate) -> PluginSubject {
        let mut state: TelemetryStateV1 = toml::from_str(IDENTIFIER_WINDOW_TEST_STATE).unwrap();
        let recording = recording_observation(&mut state);

        recording.identifier_window_scope().derive(coordinate)
    }

    #[test]
    fn plugin_scopes_round_trip_with_contract_names() {
        assert_contract_names(&[
            (PluginScope::Public, "public"),
            (PluginScope::Unnamed, "unnamed"),
            (PluginScope::Overflow, "overflow"),
        ]);
    }

    #[test]
    fn plugin_hook_vocabulary_rejects_unknown_contract_names() {
        let scope = serde_json::from_str::<PluginScope>(r#""private""#);

        assert!(scope.is_err());
    }

    #[test]
    fn plugin_hook_outcomes_follow_error_then_blocked_precedence() {
        let cases = [
            (
                PluginHookOutcomeSignals {
                    error: false,
                    blocked: false,
                },
                PluginHookOutcome::Ok,
            ),
            (
                PluginHookOutcomeSignals {
                    error: false,
                    blocked: true,
                },
                PluginHookOutcome::Blocked,
            ),
            (
                PluginHookOutcomeSignals {
                    error: true,
                    blocked: false,
                },
                PluginHookOutcome::Error,
            ),
            (
                PluginHookOutcomeSignals {
                    error: true,
                    blocked: true,
                },
                PluginHookOutcome::Error,
            ),
        ];

        for (signals, expected) in cases {
            let outcome = PluginHookOutcome::from_signals(signals);

            assert_eq!(outcome, expected);
        }
    }

    #[test]
    fn plugin_hook_outcome_counters_round_trip_in_contract_order() {
        let counters = PluginHookOutcomeCounters {
            ok: 1,
            blocked: 2,
            error: 3,
        };
        let expected = r#"{"ok":1,"blocked":2,"error":3}"#;

        let json = serde_json::to_string(&counters).unwrap();
        let decoded = serde_json::from_str::<PluginHookOutcomeCounters>(&json).unwrap();

        assert_eq!(json, expected);
        assert_eq!(decoded, counters);
    }

    #[test]
    fn recording_each_plugin_hook_outcome_increments_only_its_counter() {
        let cases = [
            (PluginHookOutcome::Ok, r#"{"ok":1,"blocked":0,"error":0}"#),
            (
                PluginHookOutcome::Blocked,
                r#"{"ok":0,"blocked":1,"error":0}"#,
            ),
            (
                PluginHookOutcome::Error,
                r#"{"ok":0,"blocked":0,"error":1}"#,
            ),
        ];

        for (outcome, expected) in cases {
            let mut counters = PluginHookOutcomeCounters::default();

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
    fn plugin_hook_outcome_counters_reject_missing_unknown_and_invalid_fields() {
        let cases = [
            r#"{"ok":0,"blocked":0}"#,
            r#"{"ok":0,"blocked":0,"error":0,"future":0}"#,
            r#"{"ok":"none","blocked":0,"error":0}"#,
            r#"{"ok":-1,"blocked":0,"error":0}"#,
        ];

        for json in cases {
            assert!(
                serde_json::from_str::<PluginHookOutcomeCounters>(json).is_err(),
                "accepted invalid plugin-hook outcome counters {json}"
            );
        }
    }

    #[test]
    fn recording_a_plugin_hook_outcome_rejects_overflow_without_mutation() {
        let outcomes = [
            PluginHookOutcome::Ok,
            PluginHookOutcome::Blocked,
            PluginHookOutcome::Error,
        ];

        for outcome in outcomes {
            let mut counters = PluginHookOutcomeCounters {
                ok: u64::MAX,
                blocked: u64::MAX,
                error: u64::MAX,
            };
            let before = counters;

            let result = counters.checked_record(outcome);

            assert_eq!(result, Err(PluginHookOutcomeCounterOverflow { outcome }));
            assert_eq!(counters, before);
        }
    }

    #[test]
    fn plugin_hook_outcome_counter_total_is_checked_for_overflow() {
        let representable = PluginHookOutcomeCounters {
            ok: 1,
            blocked: 2,
            error: 3,
        };
        let overflowing = PluginHookOutcomeCounters {
            ok: u64::MAX,
            blocked: 1,
            error: 0,
        };

        assert_eq!(representable.checked_total(), Some(6));
        assert_eq!(overflowing.checked_total(), None);
    }

    #[test]
    fn public_plugin_coordinate_is_derived_from_a_validated_plugin() {
        let extension = public_extension(
            ExtensionKind::Plugin,
            PublicExtensionSource::SymposiumRecommendations,
            "Example-tools_2",
        );

        let coordinate = PublicPluginCoordinate::try_from(&extension).unwrap();
        let json = serde_json::to_string(&coordinate).unwrap();
        let decoded = serde_json::from_str::<PublicPluginCoordinate>(&json).unwrap();

        assert_eq!(
            json,
            r#"{"source":"symposium-recommendations","name":"Example-tools_2"}"#
        );
        assert_eq!(decoded, coordinate);
        assert_eq!(
            coordinate.source(),
            PublicExtensionSource::SymposiumRecommendations
        );
        assert_eq!(coordinate.name().as_str(), "Example-tools_2");
    }

    #[test]
    fn public_plugin_coordinate_rejects_a_skill_coordinate() {
        let skill = public_extension(
            ExtensionKind::Skill,
            PublicExtensionSource::SymposiumRecommendations,
            "example-debugging",
        );

        let result = PublicPluginCoordinate::try_from(&skill);

        assert_eq!(
            result,
            Err(NotPublicPlugin {
                found: ExtensionKind::Skill,
            })
        );
    }

    #[test]
    fn public_plugin_coordinate_rejects_invalid_or_unknown_fields() {
        let invalid_raw =
            PublicPluginCoordinate::try_new(PublicExtensionSource::CratesIo, "example.tools");
        let invalid_name = serde_json::from_str::<PublicPluginCoordinate>(
            r#"{"source":"crates-io","name":"example.tools"}"#,
        );
        let unknown_field = serde_json::from_str::<PublicPluginCoordinate>(
            r#"{"source":"crates-io","name":"example-tools","type":"plugin"}"#,
        );
        let missing_field =
            serde_json::from_str::<PublicPluginCoordinate>(r#"{"source":"crates-io"}"#);

        assert_eq!(
            invalid_raw,
            Err(PublicExtensionNameError::UnsupportedCharacter)
        );
        assert!(invalid_name.is_err());
        assert!(unknown_field.is_err());
        assert!(missing_field.is_err());
    }

    #[test]
    fn plugin_subject_dimension_uses_source_then_name() {
        let coordinate = public_plugin(
            PublicExtensionSource::SymposiumRecommendations,
            "example-tools",
        );
        let expected = [
            [0, 0, 0, 0, 0, 0, 0, 25].as_slice(),
            b"symposium-recommendations".as_slice(),
            [0, 0, 0, 0, 0, 0, 0, 13].as_slice(),
            b"example-tools".as_slice(),
        ]
        .concat();

        let encoded = encode_dimension_for_test(&coordinate);

        assert_eq!(encoded, expected);
    }

    #[test]
    fn plugin_subject_derivation_matches_independent_vector() {
        let coordinate = public_plugin(
            PublicExtensionSource::SymposiumRecommendations,
            "example-tools",
        );
        // Cross-checked with .NET's HMACSHA256 over the contract header,
        // identifier window, public source, and plugin name. The complete
        // digest is
        // 7af980f6c5e53598991b1ac981cffb14c5b48919349bcd2494961fb512f74d71.
        let expected = "plg_7af980f6c5e53598991b1ac981cffb14".parse().unwrap();

        let subject = plugin_subject(&coordinate);

        assert_eq!(subject, expected);
    }

    #[test]
    fn plugin_subject_changes_with_source_or_name() {
        let baseline = public_plugin(
            PublicExtensionSource::SymposiumRecommendations,
            "example-tools",
        );
        let other_source = public_plugin(PublicExtensionSource::CratesIo, "example-tools");
        let other_name = public_plugin(
            PublicExtensionSource::SymposiumRecommendations,
            "other-tools",
        );

        let baseline = plugin_subject(&baseline);
        let other_source = plugin_subject(&other_source);
        let other_name = plugin_subject(&other_name);

        assert_ne!(baseline, other_source);
        assert_ne!(baseline, other_name);
    }
}
