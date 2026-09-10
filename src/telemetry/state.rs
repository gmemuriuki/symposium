//! In-memory representation of private telemetry state.
#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the state schema is built before persistence uses it."
    )
)]

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};

use super::{
    identity::{IdentityKey, state_key_hex},
    schema::{CohortDay, UtcDay},
};

/// The initial schema version of `telemetry-state.toml`.
///
/// Exact rather than permissive, unlike a row's `SchemaVersion`: a row written by
/// a newer binary is classified as an unknown schema and skipped, but private
/// single-writer state must never be half-understood.
#[derive(Clone, Copy)]
struct StateVersion;

impl Serialize for StateVersion {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(1)
    }
}

impl<'de> Deserialize<'de> for StateVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let version = u64::deserialize(deserializer)?;
        if version != 1 {
            return Err(D::Error::custom(format_args!(
                "expected telemetry state version 1, found {version}"
            )));
        }

        Ok(Self)
    }
}

/// Version 1 of the complete private telemetry state file.
///
/// A field may be absent only when absence represents a real lifecycle state,
/// in which case its type records that explicitly. Once this version ships,
/// adding a required field needs a migration or a new state version; a default
/// must not silently turn malformed state into valid state.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct TelemetryStateV1 {
    version: StateVersion,
    identity: IdentityState,
}

impl TelemetryStateV1 {
    /// Create private state anchored to the day recording first needs identity.
    ///
    /// The return cohort remains absent until a session is observed.
    ///
    /// # Errors
    ///
    /// Returns an error when the operating system cannot generate a secret key.
    fn new(identifier_window_anchor: UtcDay) -> Result<Self, getrandom::Error> {
        let key = IdentityKey::generate()?;
        Ok(Self::with_key(identifier_window_anchor, key))
    }

    /// Construct state from identity material that has already been generated.
    #[must_use]
    fn with_key(identifier_window_anchor: UtcDay, key: IdentityKey) -> Self {
        Self {
            version: StateVersion,
            identity: IdentityState {
                key,
                identifier_window_anchor,
                return_cohort_anchor: None,
            },
        }
    }

    /// Observe a session on a day accepted by the monotonic clock policy.
    ///
    /// The first observed session establishes D0. An existing cohort keeps its
    /// anchor through D30; the first later observation starts another D0.
    ///
    /// Storage must call this while holding the telemetry lock, after rejecting
    /// a day before the latest-opened-day high-water mark. Any high-water
    /// advancement and this cohort transition belong to the same private-state
    /// replacement, which must complete before the `session_start` row is
    /// appended.
    ///
    /// # Errors
    ///
    /// Returns an error if `effective_day` precedes the stored cohort anchor.
    /// A conforming storage caller filters this case first; this check protects
    /// the state invariant against an incorrect caller or inconsistent state.
    fn observe_session(
        &mut self,
        effective_day: UtcDay,
    ) -> Result<CohortDay, SessionDayBeforeCohortAnchor> {
        let Some(anchor) = self.identity.return_cohort_anchor else {
            self.identity.return_cohort_anchor = Some(effective_day);
            return Ok(CohortDay::D0);
        };

        let elapsed_days = effective_day.days_since(anchor);
        if elapsed_days < 0 {
            return Err(SessionDayBeforeCohortAnchor {
                observed_day: effective_day,
                cohort_anchor: anchor,
            });
        }

        if elapsed_days > i64::from(CohortDay::D30.get()) {
            self.identity.return_cohort_anchor = Some(effective_day);
            return Ok(CohortDay::D0);
        }

        Ok(CohortDay::try_from(elapsed_days)
            .expect("BUG: a session between D0 and D30 must have a valid cohort day"))
    }
}

/// An observed session day earlier than its stored return-cohort anchor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SessionDayBeforeCohortAnchor {
    observed_day: UtcDay,
    cohort_anchor: UtcDay,
}

impl fmt::Display for SessionDayBeforeCohortAnchor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "observed session day {} precedes the return-cohort anchor {}",
            self.observed_day, self.cohort_anchor
        )
    }
}

impl std::error::Error for SessionDayBeforeCohortAnchor {}

/// Stable identity material and the dates that define its rotation windows.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct IdentityState {
    #[serde(with = "state_key_hex")]
    key: IdentityKey,
    identifier_window_anchor: UtcDay,
    #[serde(skip_serializing_if = "Option::is_none")]
    return_cohort_anchor: Option<UtcDay>,
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;

    use chrono::NaiveDate;

    use super::{CohortDay, IdentityKey, TelemetryStateV1, UtcDay};

    const KEY: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    const GENERATED_KEY_BYTE: u8 = 0x42;
    /// Lowercase hexadecimal encoding of 32 [`GENERATED_KEY_BYTE`] bytes.
    const GENERATED_KEY: &str = "4242424242424242424242424242424242424242424242424242424242424242";

    fn day(year: i32, month: u32, day: u32) -> UtcDay {
        UtcDay::from_date(NaiveDate::from_ymd_opt(year, month, day).unwrap())
    }

    fn state_with_return_cohort(key: &str) -> String {
        format!(
            "version = 1\n\n[identity]\nkey = \"{key}\"\nidentifier-window-anchor = \"2026-09-10\"\nreturn-cohort-anchor = \"2026-08-11\"\n"
        )
    }

    fn state_without_return_cohort(key: &str) -> String {
        format!(
            "version = 1\n\n[identity]\nkey = \"{key}\"\nidentifier-window-anchor = \"2026-09-10\"\n"
        )
    }

    /// Rejection text for `source`, so each test can pin the reason it failed.
    ///
    /// Destructures rather than calling `unwrap_err`, which would need `Debug` on
    /// the state: the identity key deliberately has no formatting traits.
    fn rejection_message(source: &str) -> String {
        let Err(error) = toml::from_str::<TelemetryStateV1>(source) else {
            panic!("accepted invalid telemetry state:\n{source}");
        };

        error.to_string()
    }

    #[test]
    fn version_one_state_round_trips_in_canonical_form() {
        let source = state_with_return_cohort(KEY);

        let state: TelemetryStateV1 = toml::from_str(&source).unwrap();
        let serialized = toml::to_string_pretty(&state).unwrap();

        assert_eq!(serialized, source);
    }

    #[test]
    fn new_state_starts_an_identity_window_without_a_return_cohort() {
        let day = day(2026, 9, 10);

        let state = TelemetryStateV1::new(day).unwrap();

        assert_eq!(state.identity.identifier_window_anchor, day);
        assert!(state.identity.return_cohort_anchor.is_none());
    }

    #[test]
    fn generated_state_has_the_canonical_initial_file_shape() {
        let key = IdentityKey::generate_with::<Infallible>(|bytes| {
            bytes.fill(GENERATED_KEY_BYTE);
            Ok(())
        })
        .unwrap();

        let state = TelemetryStateV1::with_key(day(2026, 9, 10), key);
        let serialized = toml::to_string_pretty(&state).unwrap();

        let expected = state_without_return_cohort(GENERATED_KEY);
        assert_eq!(serialized, expected);
    }

    #[test]
    fn state_without_an_observed_session_has_no_return_cohort() {
        let source = state_without_return_cohort(KEY);

        let state: TelemetryStateV1 = toml::from_str(&source).unwrap();
        let serialized = toml::to_string_pretty(&state).unwrap();

        assert!(state.identity.return_cohort_anchor.is_none());
        assert_eq!(serialized, source);
    }

    #[test]
    fn first_observed_session_starts_d0() {
        let source = state_without_return_cohort(KEY);
        let mut state: TelemetryStateV1 = toml::from_str(&source).unwrap();
        let observed_day = day(2026, 9, 10);

        let cohort_day = state.observe_session(observed_day).unwrap();

        assert_eq!(cohort_day, CohortDay::D0);
        assert_eq!(state.identity.return_cohort_anchor, Some(observed_day));
    }

    #[test]
    fn observations_through_d30_keep_the_existing_cohort() {
        let source = state_with_return_cohort(KEY);
        let mut state: TelemetryStateV1 = toml::from_str(&source).unwrap();
        let anchor = day(2026, 8, 11);

        for (observed_day, expected_day) in [
            (day(2026, 8, 11), 0_i64),
            (day(2026, 8, 12), 1),
            (day(2026, 9, 10), 30),
        ] {
            let cohort_day = state.observe_session(observed_day).unwrap();

            assert_eq!(cohort_day, CohortDay::try_from(expected_day).unwrap());
            assert_eq!(state.identity.return_cohort_anchor, Some(anchor));
        }
    }

    #[test]
    fn first_observation_after_d30_starts_a_new_cohort() {
        let source = state_with_return_cohort(KEY);
        let mut state: TelemetryStateV1 = toml::from_str(&source).unwrap();
        let observed_day = day(2026, 9, 11);

        let cohort_day = state.observe_session(observed_day).unwrap();

        assert_eq!(cohort_day, CohortDay::D0);
        assert_eq!(state.identity.return_cohort_anchor, Some(observed_day));
    }

    #[test]
    fn observation_before_the_cohort_anchor_is_rejected_without_mutation() {
        let source = state_with_return_cohort(KEY);
        let mut state: TelemetryStateV1 = toml::from_str(&source).unwrap();
        let original_anchor = state.identity.return_cohort_anchor;

        let error = state.observe_session(day(2026, 8, 10)).unwrap_err();

        assert_eq!(
            error.to_string(),
            "observed session day 2026-08-10 precedes the return-cohort anchor 2026-08-11"
        );
        assert_eq!(state.identity.return_cohort_anchor, original_anchor);
    }

    #[test]
    fn future_state_version_is_rejected() {
        let source = state_with_return_cohort(KEY).replacen("version = 1", "version = 2", 1);

        let message = rejection_message(&source);

        assert!(
            message.contains("expected telemetry state version 1, found 2"),
            "unexpected rejection reason: {message}"
        );
    }

    #[test]
    fn unknown_top_level_field_is_rejected() {
        let source = state_with_return_cohort(KEY).replacen(
            "\n[identity]",
            "\nunexpected = true\n\n[identity]",
            1,
        );

        let message = rejection_message(&source);

        assert!(
            message.contains("unknown field `unexpected`"),
            "unexpected rejection reason: {message}"
        );
    }

    #[test]
    fn unknown_identity_field_is_rejected() {
        let mut source = state_with_return_cohort(KEY);
        source.push_str("unexpected = true\n");

        let message = rejection_message(&source);

        assert!(
            message.contains("unknown field `unexpected`"),
            "unexpected rejection reason: {message}"
        );
    }

    #[test]
    fn identity_key_must_have_exactly_64_digits() {
        let one_short = &KEY[..KEY.len() - 1];
        let one_long = format!("{KEY}0");

        for key in ["", one_short, &one_long] {
            let message = rejection_message(&state_with_return_cohort(key));

            assert!(
                message.contains("exactly 64 hexadecimal digits"),
                "accepted or misreported a {}-digit key: {message}",
                key.len()
            );
        }
    }

    #[test]
    fn identity_key_must_use_lowercase_hexadecimal() {
        for key in [KEY.to_uppercase(), KEY.replacen('f', "g", 1)] {
            let message = rejection_message(&state_with_return_cohort(&key));

            assert!(
                message.contains("lowercase hexadecimal digits"),
                "accepted or misreported {key}: {message}"
            );
        }
    }

    #[test]
    fn identity_anchor_must_be_a_canonical_utc_day() {
        let source = state_with_return_cohort(KEY).replacen("2026-09-10", "2026-9-10", 1);

        let message = rejection_message(&source);

        assert!(
            message.contains("UTC day"),
            "unexpected rejection reason: {message}"
        );
    }
}
