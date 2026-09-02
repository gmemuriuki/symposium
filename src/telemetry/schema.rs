//! Types that define telemetry's serialized data contract.
#![cfg_attr(
    not(test),
    expect(dead_code, reason = "the new schema is built before storage uses it.")
)]
use chrono::{DateTime, NaiveDate, SecondsFormat, Timelike, Utc};
use semver::Version;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};
use uuid::Uuid;

/// Random identifier for one telemetry row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub(super) struct EventId(Uuid);

impl EventId {
    /// Generate a new random version 4 UUID.
    pub(super) fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

/// UTC calendar day used to partition telemetry rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) struct UtcDay(NaiveDate);

impl Serialize for UtcDay {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(&self.0.format("%Y-%m-%d"))
    }
}

impl<'de> Deserialize<'de> for UtcDay {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        let date = NaiveDate::parse_from_str(&value, "%Y-%m-%d").map_err(D::Error::custom)?;

        if date.format("%Y-%m-%d").to_string() != value {
            return Err(D::Error::custom("expected a UTC day in YYYY-MM-DD form"));
        }
        Ok(Self(date))
    }
}

/// RFC 3339 UTC timestamp with no subsecond precision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) struct UtcSecond(DateTime<Utc>);

impl UtcSecond {
    /// Convert a UTC timestamp, discarding any subsecond precision.
    pub(super) fn from_datetime(timestamp: DateTime<Utc>) -> Self {
        Self(
            timestamp
                .with_nanosecond(0)
                .expect("BUG: zero nanoseconds must be valid for a UTC timestamp"),
        )
    }

    /// Return the UTC calendar day containing this timestamp.
    pub(super) fn day(&self) -> UtcDay {
        UtcDay(self.0.date_naive())
    }
}

impl Serialize for UtcSecond {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0.to_rfc3339_opts(SecondsFormat::Secs, true))
    }
}

impl<'de> Deserialize<'de> for UtcSecond {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        let timestamp = DateTime::parse_from_rfc3339(&value).map_err(D::Error::custom)?;

        if timestamp.offset().local_minus_utc() != 0 {
            return Err(D::Error::custom("expected a UTC timestamp"));
        }

        let canonical_z = timestamp.to_rfc3339_opts(SecondsFormat::Secs, true);
        let canonical_offset = timestamp.to_rfc3339_opts(SecondsFormat::Secs, false);

        if value != canonical_z && value != canonical_offset {
            return Err(D::Error::custom(
                "expected a UTC timestamp with whole-second precision",
            ));
        }

        Ok(Self(timestamp.with_timezone(&Utc)))
    }
}

/// Version of Symposium that produced a telemetry row.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) struct SymposiumVersion(Version);

impl SymposiumVersion {
    /// Return the version of the running Symposium binary.
    pub(super) fn current() -> Self {
        Self(
            Version::parse(env!("CARGO_PKG_VERSION"))
                .expect("BUG: Cargo package version must be valid semantic versioning"),
        )
    }
}

impl Serialize for SymposiumVersion {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for SymposiumVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Version::parse(&value).map(Self).map_err(D::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_event_id_is_uuid_v4() {
        let event_id = EventId::new();

        assert_eq!(event_id.0.get_version(), Some(uuid::Version::Random));
    }

    #[test]
    fn event_id_serializes_as_uuid_string() {
        let uuid = Uuid::parse_str("9f2c41b6-495e-4c88-a22b-c597f8102aed").unwrap();
        let event_id = EventId(uuid);

        let json = serde_json::to_string(&event_id).unwrap();

        assert_eq!(json, r#""9f2c41b6-495e-4c88-a22b-c597f8102aed""#);
    }

    #[test]
    fn event_id_round_trips_through_json() {
        let event_id = EventId::new();

        let json = serde_json::to_string(&event_id).unwrap();
        let decoded = serde_json::from_str::<EventId>(&json).unwrap();

        assert_eq!(decoded, event_id);
    }

    #[test]
    fn event_id_rejects_invalid_uuid() {
        let result = serde_json::from_str::<EventId>(r#""not-a-uuid""#);

        assert!(result.is_err());
    }

    #[test]
    fn utc_day_serializes_as_calendar_date() {
        let day = UtcDay(NaiveDate::from_ymd_opt(2026, 8, 3).unwrap());

        let json = serde_json::to_string(&day).unwrap();

        assert_eq!(json, r#""2026-08-03""#);
    }

    #[test]
    fn utc_day_round_trips_through_json() {
        let day = UtcDay(NaiveDate::from_ymd_opt(2026, 8, 3).unwrap());

        let json = serde_json::to_string(&day).unwrap();
        let decoded = serde_json::from_str::<UtcDay>(&json).unwrap();

        assert_eq!(decoded, day);
    }

    #[test]
    fn utc_day_rejects_invalid_calendar_date() {
        let result = serde_json::from_str::<UtcDay>(r#""2026-02-30""#);

        assert!(result.is_err());
    }

    #[test]
    fn utc_second_constructor_removes_subsecond_precision() {
        let timestamp = DateTime::parse_from_rfc3339("2026-08-03T09:14:02.987Z")
            .unwrap()
            .with_timezone(&Utc);

        let utc_second = UtcSecond::from_datetime(timestamp);

        assert_eq!(utc_second.0.nanosecond(), 0);
    }

    #[test]
    fn utc_second_serializes_as_canonical_utc() {
        let timestamp = DateTime::parse_from_rfc3339("2026-08-03T09:14:02Z")
            .unwrap()
            .with_timezone(&Utc);
        let utc_second = UtcSecond::from_datetime(timestamp);

        let json = serde_json::to_string(&utc_second).unwrap();

        assert_eq!(json, r#""2026-08-03T09:14:02Z""#);
    }

    #[test]
    fn utc_second_accepts_zero_offset() {
        let utc_second =
            serde_json::from_str::<UtcSecond>(r#""2026-08-03T09:14:02+00:00""#).unwrap();

        let json = serde_json::to_string(&utc_second).unwrap();

        assert_eq!(json, r#""2026-08-03T09:14:02Z""#);
    }

    #[test]
    fn utc_second_rejects_fractional_precision() {
        let result = serde_json::from_str::<UtcSecond>(r#""2026-08-03T09:14:02.000Z""#);

        assert!(result.is_err());
    }

    #[test]
    fn utc_second_rejects_non_utc_offset() {
        let result = serde_json::from_str::<UtcSecond>(r#""2026-08-03T12:14:02+03:00""#);

        assert!(result.is_err());
    }

    #[test]
    fn utc_second_rejects_unknown_local_offset() {
        let result = serde_json::from_str::<UtcSecond>(r#""2026-08-03T09:14:02-00:00""#);

        assert!(result.is_err());
    }

    #[test]
    fn utc_second_returns_its_utc_day() {
        let utc_second = serde_json::from_str::<UtcSecond>(r#""2026-08-03T23:59:59Z""#).unwrap();

        assert_eq!(
            utc_second.day(),
            UtcDay(NaiveDate::from_ymd_opt(2026, 8, 3).unwrap())
        );
    }

    #[test]
    fn current_symposium_version_uses_package_version() {
        let version = SymposiumVersion::current();

        assert_eq!(version.0.to_string(), env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn symposium_version_serializes_as_semver_string() {
        let version = SymposiumVersion(Version::new(1, 2, 3));

        let json = serde_json::to_string(&version).unwrap();

        assert_eq!(json, r#""1.2.3""#);
    }

    #[test]
    fn symposium_version_round_trips_prerelease_and_build_metadata() {
        let version = SymposiumVersion(Version::parse("1.2.3-beta.1+build.7").unwrap());

        let json = serde_json::to_string(&version).unwrap();
        let decoded = serde_json::from_str::<SymposiumVersion>(&json).unwrap();

        assert_eq!(decoded, version);
    }

    #[test]
    fn symposium_version_rejects_invalid_semver() {
        let result = serde_json::from_str::<SymposiumVersion>(r#""not-a-version""#);

        assert!(result.is_err());
    }
}
