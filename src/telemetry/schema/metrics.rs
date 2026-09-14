//! Shared schema types for cumulative telemetry metrics.

use std::{fmt, time::Duration};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};

const LATENCY_BOUNDS_MS: [u64; 8] = [5, 10, 25, 50, 100, 250, 500, 1_000];
const LATENCY_BUCKET_COUNT: usize = LATENCY_BOUNDS_MS.len() + 1;

/// Maximum number of distinct sessions retained by one hook aggregate.
pub(in crate::telemetry) const MAX_IDENTIFIED_SESSIONS: u64 = 256;

/// Convert a duration to the contract's whole-millisecond representation.
///
/// Sub-millisecond precision is truncated. Durations outside the wire type's
/// range saturate instead of wrapping to an unrelated smaller value.
#[must_use]
pub(in crate::telemetry) fn duration_millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

/// Fixed millisecond histogram shared by cumulative telemetry rows.
///
/// The bounds are part of the version 1 wire contract rather than runtime
/// state. Keeping only the counters in this type makes changed bounds
/// unrepresentable after deserialization.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(in crate::telemetry) struct LatencyHistogram {
    counts: [u64; LATENCY_BUCKET_COUNT],
}

impl LatencyHistogram {
    /// Increment the bucket containing `duration` after converting it to the
    /// contract's whole-millisecond representation.
    ///
    /// # Errors
    ///
    /// Returns [`LatencyHistogramError::BucketCountOverflow`] when the
    /// selected counter cannot be incremented. The histogram is unchanged on
    /// failure.
    #[must_use = "counter overflow must drop the containing telemetry update"]
    pub(in crate::telemetry) fn checked_record(
        &mut self,
        duration: Duration,
    ) -> Result<(), LatencyHistogramError> {
        let duration_ms = duration_millis(duration);
        let bucket = LATENCY_BOUNDS_MS.partition_point(|bound| duration_ms > *bound);
        let next = self.counts[bucket]
            .checked_add(1)
            .ok_or(LatencyHistogramError::BucketCountOverflow { bucket })?;

        self.counts[bucket] = next;
        Ok(())
    }

    /// Return the sum of every bucket, or `None` if the total exceeds `u64`.
    #[must_use]
    pub(in crate::telemetry) fn checked_total(&self) -> Option<u64> {
        self.counts
            .iter()
            .try_fold(0_u64, |total, count| total.checked_add(*count))
    }
}

/// A cumulative latency counter that cannot be represented in `u64`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::telemetry) enum LatencyHistogramError {
    BucketCountOverflow { bucket: usize },
}

impl fmt::Display for LatencyHistogramError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BucketCountOverflow { bucket } => {
                write!(formatter, "latency histogram bucket {bucket} overflows u64")
            }
        }
    }
}

impl std::error::Error for LatencyHistogramError {}

impl Serialize for LatencyHistogram {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        RawLatencyHistogram {
            bounds: LATENCY_BOUNDS_MS,
            counts: self.counts,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for LatencyHistogram {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawLatencyHistogram::deserialize(deserializer)?;

        if raw.bounds != LATENCY_BOUNDS_MS {
            return Err(D::Error::custom(format_args!(
                "expected latency histogram bounds {LATENCY_BOUNDS_MS:?}, found {:?}",
                raw.bounds
            )));
        }

        Ok(Self { counts: raw.counts })
    }
}

/// Strict wire representation of a latency histogram.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLatencyHistogram {
    bounds: [u64; LATENCY_BOUNDS_MS.len()],
    counts: [u64; LATENCY_BUCKET_COUNT],
}

/// Inputs to the session-count rules shared by hook aggregate rows.
///
/// A named input keeps the two optional counters and the two observation
/// totals from being transposed at call sites.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::telemetry) struct HookSessionCountInput {
    pub(in crate::telemetry) counts_complete: bool,
    pub(in crate::telemetry) identified_sessions: Option<u64>,
    pub(in crate::telemetry) identified_sessions_non_ok: Option<u64>,
    pub(in crate::telemetry) invocations: u64,
    pub(in crate::telemetry) ok_invocations: u64,
}

/// Validate the complete-session rules shared by hook aggregate rows.
///
/// # Errors
///
/// Returns [`HookSessionCountError`] when the presence or value of a session
/// counter is inconsistent with the aggregate's invocation counters.
pub(in crate::telemetry) fn validate_hook_session_counts(
    input: HookSessionCountInput,
) -> Result<(), HookSessionCountError> {
    match (
        input.counts_complete,
        input.identified_sessions,
        input.identified_sessions_non_ok,
    ) {
        (true, Some(identified), Some(non_ok)) => {
            if identified == 0 {
                return Err(HookSessionCountError::NoIdentifiedSessions);
            }
            if identified > MAX_IDENTIFIED_SESSIONS {
                return Err(HookSessionCountError::IdentifiedSessionsExceedLimit {
                    identified,
                    maximum: MAX_IDENTIFIED_SESSIONS,
                });
            }
            if identified > input.invocations {
                return Err(HookSessionCountError::IdentifiedSessionsExceedInvocations {
                    identified,
                    invocations: input.invocations,
                });
            }
            if non_ok > identified {
                return Err(HookSessionCountError::NonOkSessionsExceedIdentified {
                    identified,
                    non_ok,
                });
            }

            let non_ok_invocations = input.invocations.checked_sub(input.ok_invocations).ok_or(
                HookSessionCountError::OkInvocationsExceedInvocations {
                    invocations: input.invocations,
                    ok_invocations: input.ok_invocations,
                },
            )?;
            if non_ok_invocations > 0 && non_ok == 0 {
                return Err(HookSessionCountError::NoNonOkSessions {
                    invocations: non_ok_invocations,
                });
            }
            if non_ok > non_ok_invocations {
                return Err(HookSessionCountError::NonOkSessionsExceedNonOkInvocations {
                    sessions: non_ok,
                    invocations: non_ok_invocations,
                });
            }

            // The subset check above proves that this subtraction cannot
            // underflow. Every remaining session contributed an `ok` result.
            let all_ok_sessions = identified - non_ok;
            if all_ok_sessions > input.ok_invocations {
                return Err(HookSessionCountError::AllOkSessionsExceedOkInvocations {
                    sessions: all_ok_sessions,
                    invocations: input.ok_invocations,
                });
            }

            Ok(())
        }
        (false, None, None) => Ok(()),
        (true, _, _) => Err(HookSessionCountError::CompleteCountsMissing),
        (false, _, _) => Err(HookSessionCountError::IncompleteCountsPresent),
    }
}

/// Invalid relationship between session counters in a hook aggregate row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::telemetry) enum HookSessionCountError {
    CompleteCountsMissing,
    IncompleteCountsPresent,
    NoIdentifiedSessions,
    NoNonOkSessions {
        invocations: u64,
    },
    IdentifiedSessionsExceedLimit {
        identified: u64,
        maximum: u64,
    },
    IdentifiedSessionsExceedInvocations {
        identified: u64,
        invocations: u64,
    },
    NonOkSessionsExceedIdentified {
        identified: u64,
        non_ok: u64,
    },
    OkInvocationsExceedInvocations {
        invocations: u64,
        ok_invocations: u64,
    },
    NonOkSessionsExceedNonOkInvocations {
        sessions: u64,
        invocations: u64,
    },
    AllOkSessionsExceedOkInvocations {
        sessions: u64,
        invocations: u64,
    },
}

impl fmt::Display for HookSessionCountError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CompleteCountsMissing => formatter
                .write_str("complete hook session counts require both identified session counters"),
            Self::IncompleteCountsPresent => formatter.write_str(
                "incomplete hook session counts must omit both identified session counters",
            ),
            Self::NoIdentifiedSessions => {
                formatter.write_str("complete hook session counts contain no identified sessions")
            }
            Self::NoNonOkSessions { invocations } => write!(
                formatter,
                "{invocations} non-ok hook invocations require at least one non-ok identified session"
            ),
            Self::IdentifiedSessionsExceedLimit {
                identified,
                maximum,
            } => write!(
                formatter,
                "identified sessions {identified} exceed the version 1 limit {maximum}"
            ),
            Self::IdentifiedSessionsExceedInvocations {
                identified,
                invocations,
            } => write!(
                formatter,
                "identified sessions {identified} exceed {invocations} hook invocations"
            ),
            Self::NonOkSessionsExceedIdentified { identified, non_ok } => write!(
                formatter,
                "non-ok identified sessions {non_ok} exceed {identified} identified sessions"
            ),
            Self::OkInvocationsExceedInvocations {
                invocations,
                ok_invocations,
            } => write!(
                formatter,
                "ok hook invocations {ok_invocations} exceed {invocations} hook invocations"
            ),
            Self::NonOkSessionsExceedNonOkInvocations {
                sessions,
                invocations,
            } => write!(
                formatter,
                "non-ok identified sessions {sessions} exceed {invocations} non-ok hook invocations"
            ),
            Self::AllOkSessionsExceedOkInvocations {
                sessions,
                invocations,
            } => write!(
                formatter,
                "all-ok identified sessions {sessions} exceed {invocations} ok hook invocations"
            ),
        }
    }
}

impl std::error::Error for HookSessionCountError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_histogram_matches_the_recorded_data_contract() {
        let example = super::super::recorded_data_example_block(
            "### Rules shared by hook aggregates",
            "```json",
        )
        .trim();

        let histogram = serde_json::from_str::<LatencyHistogram>(example).unwrap();
        let encoded = serde_json::to_string(&histogram).unwrap();

        assert_eq!(histogram, LatencyHistogram::default());
        assert_eq!(encoded, example);
    }

    #[test]
    fn durations_use_the_contract_bucket_boundaries() {
        let mut histogram = LatencyHistogram::default();
        let durations_ms = [
            0,
            5,
            6,
            10,
            11,
            25,
            26,
            50,
            51,
            100,
            101,
            250,
            251,
            500,
            501,
            1_000,
            1_001,
            u64::MAX,
        ];

        for duration_ms in durations_ms {
            histogram
                .checked_record(Duration::from_millis(duration_ms))
                .unwrap();
        }

        assert_eq!(histogram.counts, [2; LATENCY_BUCKET_COUNT]);
    }

    #[test]
    fn duration_conversion_truncates_and_saturates() {
        let sub_millisecond = Duration::from_nanos(999_999);
        let fractional_millisecond = Duration::from_micros(5_999);
        let outside_u64_milliseconds = Duration::from_secs(u64::MAX / 1_000 + 1);

        assert_eq!(duration_millis(sub_millisecond), 0);
        assert_eq!(duration_millis(fractional_millisecond), 5);
        assert_eq!(duration_millis(outside_u64_milliseconds), u64::MAX);
    }

    #[test]
    fn histogram_rejects_changed_bounds() {
        let json = r#"{"bounds":[4,10,25,50,100,250,500,1000],"counts":[0,0,0,0,0,0,0,0,0]}"#;

        let error = serde_json::from_str::<LatencyHistogram>(json).unwrap_err();

        assert!(
            error
                .to_string()
                .contains("expected latency histogram bounds"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn histogram_rejects_variable_array_lengths() {
        let cases = [
            r#"{"bounds":[5,10,25,50,100,250,500],"counts":[0,0,0,0,0,0,0,0,0]}"#,
            r#"{"bounds":[5,10,25,50,100,250,500,1000,2000],"counts":[0,0,0,0,0,0,0,0,0]}"#,
            r#"{"bounds":[5,10,25,50,100,250,500,1000],"counts":[0,0,0,0,0,0,0,0]}"#,
            r#"{"bounds":[5,10,25,50,100,250,500,1000],"counts":[0,0,0,0,0,0,0,0,0,0]}"#,
        ];

        for json in cases {
            assert!(
                serde_json::from_str::<LatencyHistogram>(json).is_err(),
                "accepted variable histogram shape {json}"
            );
        }
    }

    #[test]
    fn histogram_rejects_missing_and_unknown_fields() {
        let cases = [
            r#"{"counts":[0,0,0,0,0,0,0,0,0]}"#,
            r#"{"bounds":[5,10,25,50,100,250,500,1000]}"#,
            r#"{"bounds":[5,10,25,50,100,250,500,1000],"counts":[0,0,0,0,0,0,0,0,0],"future":true}"#,
        ];

        for json in cases {
            assert!(
                serde_json::from_str::<LatencyHistogram>(json).is_err(),
                "accepted invalid histogram object {json}"
            );
        }
    }

    #[test]
    fn recording_rejects_overflow_without_mutation() {
        let mut histogram = LatencyHistogram {
            counts: [u64::MAX, 1, 2, 3, 4, 5, 6, 7, 8],
        };
        let before = histogram;

        let result = histogram.checked_record(Duration::from_millis(5));

        assert_eq!(
            result,
            Err(LatencyHistogramError::BucketCountOverflow { bucket: 0 })
        );
        assert_eq!(histogram, before);
    }

    #[test]
    fn histogram_total_is_checked_for_overflow() {
        let representable = LatencyHistogram {
            counts: [1, 2, 3, 4, 5, 6, 7, 8, 9],
        };
        let overflowing = LatencyHistogram {
            counts: [u64::MAX, 1, 0, 0, 0, 0, 0, 0, 0],
        };

        assert_eq!(representable.checked_total(), Some(45));
        assert_eq!(overflowing.checked_total(), None);
    }

    #[test]
    fn session_counts_reject_more_ok_than_total_invocations() {
        let input = HookSessionCountInput {
            counts_complete: true,
            identified_sessions: Some(1),
            identified_sessions_non_ok: Some(0),
            invocations: 1,
            ok_invocations: 2,
        };

        let result = validate_hook_session_counts(input);

        assert_eq!(
            result,
            Err(HookSessionCountError::OkInvocationsExceedInvocations {
                invocations: 1,
                ok_invocations: 2,
            })
        );
    }
}
