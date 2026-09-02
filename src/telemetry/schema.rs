//! Types that define telemetry's serialized data contract.
#![cfg_attr(
    not(test),
    expect(dead_code, reason = "the new schema is built before storage uses it.")
)]
use serde::{Deserialize, Serialize};
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
}
