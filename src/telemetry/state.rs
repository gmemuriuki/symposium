//! In-memory representation of private telemetry state.
#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the state schema is built before persistence uses it."
    )
)]

use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};

use super::{
    identity::{IdentityKey, state_key_hex},
    schema::UtcDay,
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
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct TelemetryStateV1 {
    version: StateVersion,
    identity: IdentityState,
}

/// Stable identity material and the dates that define its rotation windows.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct IdentityState {
    #[serde(with = "state_key_hex")]
    key: IdentityKey,
    identifier_window_anchor: UtcDay,
    return_cohort_anchor: UtcDay,
}

#[cfg(test)]
mod tests {
    use super::TelemetryStateV1;

    const KEY: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn state_with_key(key: &str) -> String {
        format!(
            "version = 1\n\n[identity]\nkey = \"{key}\"\nidentifier-window-anchor = \"2026-09-10\"\nreturn-cohort-anchor = \"2026-08-11\"\n"
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
        let source = state_with_key(KEY);

        let state: TelemetryStateV1 = toml::from_str(&source).unwrap();
        let serialized = toml::to_string_pretty(&state).unwrap();

        assert_eq!(serialized, source);
    }

    #[test]
    fn future_state_version_is_rejected() {
        let source = state_with_key(KEY).replacen("version = 1", "version = 2", 1);

        let message = rejection_message(&source);

        assert!(
            message.contains("expected telemetry state version 1, found 2"),
            "unexpected rejection reason: {message}"
        );
    }

    #[test]
    fn unknown_top_level_field_is_rejected() {
        let source =
            state_with_key(KEY).replacen("\n[identity]", "\nunexpected = true\n\n[identity]", 1);

        let message = rejection_message(&source);

        assert!(
            message.contains("unknown field `unexpected`"),
            "unexpected rejection reason: {message}"
        );
    }

    #[test]
    fn unknown_identity_field_is_rejected() {
        let mut source = state_with_key(KEY);
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
            let message = rejection_message(&state_with_key(key));

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
            let message = rejection_message(&state_with_key(&key));

            assert!(
                message.contains("lowercase hexadecimal digits"),
                "accepted or misreported {key}: {message}"
            );
        }
    }

    #[test]
    fn identity_anchor_must_be_a_canonical_utc_day() {
        let source = state_with_key(KEY).replacen("2026-09-10", "2026-9-10", 1);

        let message = rejection_message(&source);

        assert!(
            message.contains("UTC day"),
            "unexpected rejection reason: {message}"
        );
    }
}
