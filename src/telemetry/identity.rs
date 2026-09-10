//! Domain-specific identifiers used by telemetry rows.
#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "identifier types are built before telemetry producers use them."
    )
)]

use std::{
    cmp::Ordering,
    fmt,
    hash::{Hash, Hasher},
    marker::PhantomData,
    str::FromStr,
};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};

const IDENTIFIER_BYTES: usize = 16;
const ENCODED_DIGITS: usize = IDENTIFIER_BYTES * 2;

/// A 128-bit telemetry identifier belonging to domain `D`.
///
/// Its wire form is the domain prefix followed by `ENCODED_DIGITS` lowercase
/// hexadecimal digits.
pub(super) struct ScopedId<D> {
    bytes: [u8; IDENTIFIER_BYTES],
    domain: PhantomData<D>,
}

impl<D> ScopedId<D> {
    /// Wrap the leading 128 bits of a derived pseudonym.
    ///
    /// Private, so identifier derivation has to live in this module rather than
    /// anywhere in telemetry that happens to hold sixteen bytes.
    #[must_use]
    const fn from_bytes(bytes: [u8; IDENTIFIER_BYTES]) -> Self {
        Self {
            bytes,
            domain: PhantomData,
        }
    }
}

// Written out rather than derived: a derive puts the same bound on `D`, so an
// identifier would only gain each trait when its zero-sized marker declared it.
impl<D> Clone for ScopedId<D> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<D> Copy for ScopedId<D> {}

impl<D> PartialEq for ScopedId<D> {
    fn eq(&self, other: &Self) -> bool {
        self.bytes == other.bytes
    }
}

impl<D> Eq for ScopedId<D> {}

impl<D> PartialOrd for ScopedId<D> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<D> Ord for ScopedId<D> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.bytes.cmp(&other.bytes)
    }
}

impl<D> Hash for ScopedId<D> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.bytes.hash(state);
    }
}

/// Marker supplying a [`ScopedId`] domain's wire prefix.
///
/// Private, which seals it: the prefix set is a published contract surface, not
/// an extension point.
trait ScopedIdDomain {
    const PREFIX: &'static str;
}

/// Reason a stored scoped identifier is not canonical.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ParseScopedIdError {
    IncorrectPrefix,
    IncorrectLength,
    InvalidHex,
}

impl fmt::Display for ParseScopedIdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IncorrectPrefix => formatter.write_str("identifier has the wrong domain prefix"),
            Self::IncorrectLength => write!(
                formatter,
                "identifier must contain exactly {ENCODED_DIGITS} hexadecimal digits"
            ),
            Self::InvalidHex => {
                formatter.write_str("identifier contains a non-lowercase-hexadecimal character")
            }
        }
    }
}

impl std::error::Error for ParseScopedIdError {}

// Debug prints the wire form too: the derived one dumps sixteen decimal numbers,
// which makes a failed identifier comparison unreadable.
impl<D> fmt::Debug for ScopedId<D>
where
    D: ScopedIdDomain,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self}")
    }
}

impl<D> fmt::Display for ScopedId<D>
where
    D: ScopedIdDomain,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(D::PREFIX)?;
        for byte in self.bytes {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl<D> FromStr for ScopedId<D>
where
    D: ScopedIdDomain,
{
    type Err = ParseScopedIdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let encoded = value
            .strip_prefix(D::PREFIX)
            .ok_or(ParseScopedIdError::IncorrectPrefix)?;

        if encoded.len() != ENCODED_DIGITS {
            return Err(ParseScopedIdError::IncorrectLength);
        }

        // The length check leaves no remainder, so every digit reaches a pair.
        let (digit_pairs, _) = encoded.as_bytes().as_chunks::<2>();

        let mut bytes = [0; IDENTIFIER_BYTES];
        for (&[high, low], output) in digit_pairs.iter().zip(&mut bytes) {
            let high = decode_lower_hex(high).ok_or(ParseScopedIdError::InvalidHex)?;
            let low = decode_lower_hex(low).ok_or(ParseScopedIdError::InvalidHex)?;
            *output = (high << 4) | low;
        }

        Ok(Self::from_bytes(bytes))
    }
}

impl<D> Serialize for ScopedId<D>
where
    D: ScopedIdDomain,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(self)
    }
}

impl<'de, D> Deserialize<'de> for ScopedId<D>
where
    D: ScopedIdDomain,
{
    fn deserialize<De>(deserializer: De) -> Result<Self, De::Error>
    where
        De: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(De::Error::custom)
    }
}

fn decode_lower_hex(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        _ => None,
    }
}

/// Declares each identifier domain, its contract prefix, and its alias.
///
/// A macro because a function cannot introduce types, and the prefix is the
/// hand-written part worth handing to the tests as a table.
macro_rules! scoped_id_domains {
    ($($domain:ident => $prefix:literal as $alias:ident,)+) => {
        $(
            pub(super) enum $domain {}

            impl ScopedIdDomain for $domain {
                const PREFIX: &'static str = $prefix;
            }

            pub(super) type $alias = ScopedId<$domain>;
        )+

        #[cfg(test)]
        const PREFIX_PARSERS: &[(&str, fn(&str) -> bool)] = &[
            $(($prefix, |value| value.parse::<$alias>().is_ok())),+
        ];
    };
}

scoped_id_domains! {
    SessionDomain   => "sess_" as SessionId,
    RetentionDomain => "ret_"  as RetentionSubject,
    AgentDomain     => "agt_"  as AgentSubject,
    PackageDomain   => "pkg_"  as PackageSubject,
    ExtensionDomain => "ext_"  as ExtensionSubject,
    HookDomain      => "hok_"  as HookSubject,
    PluginDomain    => "plg_"  as PluginSubject,
    CommandDomain   => "cmd_"  as CommandSubject,
}

#[cfg(test)]
mod tests {
    use std::{any::TypeId, collections::HashSet, mem::size_of};

    use super::*;

    const RECORDED_DATA: &str =
        include_str!("../../md/rfds/telemetry-recording/contract/recorded-data.md");

    const TEST_HEX: &str = "00112233445566778899aabbccddeeff";

    const TEST_BYTES: [u8; IDENTIFIER_BYTES] = [
        0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee,
        0xff,
    ];

    enum TestDomain {}

    impl ScopedIdDomain for TestDomain {
        const PREFIX: &'static str = "test_";
    }

    /// Reads the first identifier the contract spells with `prefix`.
    fn contract_identifier(prefix: &str) -> &'static str {
        let opening_quote = RECORDED_DATA
            .find(&format!("\"{prefix}"))
            .unwrap_or_else(|| panic!("recorded-data contract names no {prefix} identifier"));

        let value = &RECORDED_DATA[opening_quote + 1..];
        let closing_quote = value
            .find('"')
            .expect("recorded-data identifier must be terminated");

        &value[..closing_quote]
    }

    #[test]
    fn domain_marker_adds_no_storage_to_identifier() {
        let bytes = [0x5a; IDENTIFIER_BYTES];

        let identifier = ScopedId::<TestDomain>::from_bytes(bytes);

        assert_eq!(identifier, ScopedId::<TestDomain>::from_bytes(bytes));
        assert_eq!(size_of::<ScopedId<TestDomain>>(), IDENTIFIER_BYTES);
    }

    #[test]
    fn identifier_domains_are_distinct_types() {
        let domains = [
            TypeId::of::<SessionId>(),
            TypeId::of::<RetentionSubject>(),
            TypeId::of::<AgentSubject>(),
            TypeId::of::<PackageSubject>(),
            TypeId::of::<ExtensionSubject>(),
            TypeId::of::<HookSubject>(),
            TypeId::of::<PluginSubject>(),
            TypeId::of::<CommandSubject>(),
        ];

        let unique_domains = domains.into_iter().collect::<HashSet<_>>();

        assert_eq!(unique_domains.len(), domains.len());
    }

    #[test]
    fn domain_prefixes_are_distinct() {
        let prefixes = PREFIX_PARSERS
            .iter()
            .map(|(prefix, _)| *prefix)
            .collect::<HashSet<_>>();

        assert_eq!(prefixes.len(), PREFIX_PARSERS.len());
    }

    #[test]
    fn each_domain_accepts_only_its_own_prefix() {
        for (prefix, parses) in PREFIX_PARSERS {
            for (candidate_prefix, _) in PREFIX_PARSERS {
                let value = format!("{candidate_prefix}{TEST_HEX}");

                assert_eq!(
                    parses(&value),
                    prefix == candidate_prefix,
                    "{prefix} domain mishandled {value}"
                );
            }
        }
    }

    #[test]
    fn contract_identifiers_parse_in_their_own_domain() {
        for (prefix, parses) in PREFIX_PARSERS {
            let identifier = contract_identifier(prefix);

            assert!(
                parses(identifier),
                "contract identifier {identifier} does not parse in the {prefix} domain"
            );
        }
    }

    #[test]
    fn identifiers_display_with_their_contract_prefixes() {
        let rendered = [
            SessionId::from_bytes(TEST_BYTES).to_string(),
            RetentionSubject::from_bytes(TEST_BYTES).to_string(),
            AgentSubject::from_bytes(TEST_BYTES).to_string(),
            PackageSubject::from_bytes(TEST_BYTES).to_string(),
            ExtensionSubject::from_bytes(TEST_BYTES).to_string(),
            HookSubject::from_bytes(TEST_BYTES).to_string(),
            PluginSubject::from_bytes(TEST_BYTES).to_string(),
            CommandSubject::from_bytes(TEST_BYTES).to_string(),
        ];

        assert_eq!(
            rendered,
            [
                "sess_00112233445566778899aabbccddeeff",
                "ret_00112233445566778899aabbccddeeff",
                "agt_00112233445566778899aabbccddeeff",
                "pkg_00112233445566778899aabbccddeeff",
                "ext_00112233445566778899aabbccddeeff",
                "hok_00112233445566778899aabbccddeeff",
                "plg_00112233445566778899aabbccddeeff",
                "cmd_00112233445566778899aabbccddeeff",
            ]
        );
    }

    #[test]
    fn identifier_debug_uses_the_wire_form() {
        let identifier = SessionId::from_bytes(TEST_BYTES);

        let debug = format!("{identifier:?}");

        assert_eq!(debug, "sess_00112233445566778899aabbccddeeff");
    }

    #[test]
    fn identifier_text_round_trips() {
        let identifier = SessionId::from_bytes(TEST_BYTES);

        let parsed = identifier.to_string().parse::<SessionId>().unwrap();

        assert_eq!(parsed, identifier);
    }

    #[test]
    fn identifier_json_round_trips() {
        let identifier = SessionId::from_bytes(TEST_BYTES);

        let json = serde_json::to_string(&identifier).unwrap();

        assert_eq!(json, format!("\"sess_{TEST_HEX}\""));
        assert_eq!(
            serde_json::from_str::<SessionId>(&json).unwrap(),
            identifier
        );
    }

    #[test]
    fn identifier_json_rejects_another_domain_prefix() {
        let json = format!("\"cmd_{TEST_HEX}\"");

        let result = serde_json::from_str::<PackageSubject>(&json);

        assert!(result.is_err());
    }

    #[test]
    fn identifier_json_rejects_non_string_values() {
        for json in ["17", "{}", "[]", "true", "null"] {
            let result = serde_json::from_str::<SessionId>(json);

            assert!(result.is_err(), "accepted non-string identifier: {json}");
        }
    }

    #[test]
    fn identifier_rejects_another_domain_prefix() {
        let value = format!("cmd_{TEST_HEX}");

        let result = value.parse::<PackageSubject>();

        assert_eq!(result, Err(ParseScopedIdError::IncorrectPrefix));
    }

    #[test]
    fn identifier_rejects_wrong_lengths() {
        let cases = [
            "sess_",
            "sess_00112233445566778899aabbccddeef",
            "sess_00112233445566778899aabbccddeeff0",
        ];

        for value in cases {
            assert_eq!(
                value.parse::<SessionId>(),
                Err(ParseScopedIdError::IncorrectLength),
                "accepted identifier with the wrong length: {value}"
            );
        }
    }

    #[test]
    fn identifier_rejects_uppercase_hexadecimal() {
        let value = "sess_00112233445566778899AABBCCDDEEFF";

        let result = value.parse::<SessionId>();

        assert_eq!(result, Err(ParseScopedIdError::InvalidHex));
    }

    #[test]
    fn identifier_rejects_non_hexadecimal_character() {
        let value = "sess_00112233445566778899aabbccddeefg";

        let result = value.parse::<SessionId>();

        assert_eq!(result, Err(ParseScopedIdError::InvalidHex));
    }
}
