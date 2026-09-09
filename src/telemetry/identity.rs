//! Domain-specific identifiers used by telemetry rows.
#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "identifier types are built before telemetry producers use them."
    )
)]

use std::marker::PhantomData;

const IDENTIFIER_BYTES: usize = 16;

/// A 128-bit telemetry identifier belonging to domain `D`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) struct ScopedId<D> {
    bytes: [u8; IDENTIFIER_BYTES],
    domain: PhantomData<D>,
}

impl<D> ScopedId<D> {
    const fn from_bytes(bytes: [u8; IDENTIFIER_BYTES]) -> Self {
        Self {
            bytes,
            domain: PhantomData,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) enum SessionIdDomain {}

pub(super) type SessionId = ScopedId<SessionIdDomain>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) enum RetentionDomain {}

pub(super) type RetentionSubject = ScopedId<RetentionDomain>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) enum AgentDomain {}

pub(super) type AgentSubject = ScopedId<AgentDomain>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) enum PackageDomain {}

pub(super) type PackageSubject = ScopedId<PackageDomain>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) enum ExtensionDomain {}

pub(super) type ExtensionSubject = ScopedId<ExtensionDomain>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) enum HookDomain {}

pub(super) type HookSubject = ScopedId<HookDomain>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) enum PluginDomain {}

pub(super) type PluginSubject = ScopedId<PluginDomain>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) enum CommandDomain {}

pub(super) type CommandSubject = ScopedId<CommandDomain>;

#[cfg(test)]
mod tests {
    use std::{any::TypeId, collections::HashSet, mem::size_of};

    use super::*;

    enum TestDomain {}

    #[test]
    fn domain_marker_adds_no_storage_to_identifier() {
        let bytes = [0x5a; IDENTIFIER_BYTES];

        let identifier = ScopedId::<TestDomain>::from_bytes(bytes);

        assert_eq!(identifier.bytes, bytes);
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
}
