//! Validation shared by public names in the telemetry contract.

/// Rule applied to the first byte of a public name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum InitialByteRule {
    Alphabetic,
    Alphanumeric,
}

/// Structural reason a public name fails its versioned grammar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PublicNameViolation {
    Empty,
    TooLong,
    InvalidInitialByte,
    UnsupportedCharacter,
}

/// Validate the common ASCII shape of a versioned public telemetry name.
pub(super) fn validate_public_name(
    value: &str,
    maximum_bytes: usize,
    initial_byte_rule: InitialByteRule,
) -> Result<(), PublicNameViolation> {
    let Some((first, rest)) = value.as_bytes().split_first() else {
        return Err(PublicNameViolation::Empty);
    };

    if value.len() > maximum_bytes {
        return Err(PublicNameViolation::TooLong);
    }

    let initial_byte_is_valid = match initial_byte_rule {
        InitialByteRule::Alphabetic => first.is_ascii_alphabetic(),
        InitialByteRule::Alphanumeric => first.is_ascii_alphanumeric(),
    };
    if !initial_byte_is_valid {
        return Err(PublicNameViolation::InvalidInitialByte);
    }

    if !rest
        .iter()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(PublicNameViolation::UnsupportedCharacter);
    }

    Ok(())
}
