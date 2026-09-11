//! Safe extension-resolution path vocabulary.

use std::fmt;

use serde::{Deserialize, Serialize};

use super::{
    super::extension::{
        ExtensionKind, PublicExtensionCoordinate, PublicExtensionName, PublicExtensionSource,
    },
    package::PublicPackageCoordinate,
};

/// Safe evidence node in a successful extension-resolution path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ResolutionPathNode {
    Package(PublicPackageCoordinate),
    Extension(ExtensionNode),
    All(AllNode),
    Any(AnyNode),
    Not(NotNode),
    Opaque(OpaqueNode),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExtensionNode {
    extension_type: ExtensionKind,
    source: PublicExtensionSource,
    name: PublicExtensionName,
}

impl From<PublicExtensionCoordinate> for ExtensionNode {
    fn from(coordinate: PublicExtensionCoordinate) -> Self {
        Self {
            extension_type: coordinate.kind(),
            source: coordinate.source(),
            name: coordinate.name().clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawAllNode")]
struct AllNode {
    children: Vec<ResolutionPathNode>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAllNode {
    children: Vec<ResolutionPathNode>,
}

impl TryFrom<RawAllNode> for AllNode {
    type Error = EmptyAllNode;

    fn try_from(raw: RawAllNode) -> Result<Self, Self::Error> {
        if raw.children.is_empty() {
            return Err(EmptyAllNode);
        }

        Ok(Self {
            children: raw.children,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EmptyAllNode;

impl fmt::Display for EmptyAllNode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("all resolution node must contain at least one child")
    }
}

impl std::error::Error for EmptyAllNode {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AnyNode {
    child: Box<ResolutionPathNode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct NotNode {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct OpaqueNode {
    reason: OpaqueResolutionReason,
}

/// Fixed explanation for resolution evidence that is unsafe to name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum OpaqueResolutionReason {
    PrivateSource,
    NonPackagePredicate,
    Limit,
}

#[cfg(test)]
mod tests {
    use super::super::super::recorded_data_example_block;
    use super::*;

    fn documented_path_node_examples() -> impl Iterator<Item = &'static str> {
        let example_block = recorded_data_example_block("### `extension_resolution`", "```json");

        example_block.lines().filter(|line| !line.is_empty())
    }

    #[test]
    fn documented_resolution_path_nodes_round_trip_in_contract_shape() {
        for json in documented_path_node_examples() {
            let node = serde_json::from_str::<ResolutionPathNode>(json).unwrap();
            let encoded = serde_json::to_string(&node).unwrap();

            assert_eq!(encoded, json);
        }
    }

    #[test]
    fn extension_node_is_built_from_a_validated_coordinate() {
        let coordinate = PublicExtensionCoordinate::try_new(
            ExtensionKind::Skill,
            PublicExtensionSource::SymposiumRecommendations,
            "example-debugging",
        )
        .unwrap();

        let node = ResolutionPathNode::Extension(coordinate.into());
        let encoded = serde_json::to_string(&node).unwrap();

        assert_eq!(
            encoded,
            r#"{"type":"extension","extension_type":"skill","source":"symposium-recommendations","name":"example-debugging"}"#
        );
    }

    #[test]
    fn resolution_path_nodes_reject_unknown_nested_fields() {
        let json = r#"{"type":"any","child":{"type":"not","predicate":"private"}}"#;

        let result = serde_json::from_str::<ResolutionPathNode>(json);

        assert!(result.is_err());
    }

    #[test]
    fn resolution_path_nodes_require_their_contract_fields() {
        let json = r#"{"type":"extension","extension_type":"skill","name":"example-debugging"}"#;

        let result = serde_json::from_str::<ResolutionPathNode>(json);

        assert!(result.is_err());
    }

    #[test]
    fn all_resolution_node_requires_at_least_one_child() {
        let json = r#"{"type":"all","children":[]}"#;

        let result = serde_json::from_str::<ResolutionPathNode>(json);

        assert!(result.is_err());
    }

    #[test]
    fn resolution_path_nodes_reject_unknown_contract_vocabulary() {
        let unknown_type = serde_json::from_str::<ResolutionPathNode>(r#"{"type":"custom"}"#);
        let unknown_reason = serde_json::from_str::<ResolutionPathNode>(
            r#"{"type":"opaque","reason":"private_predicate"}"#,
        );

        assert!(unknown_type.is_err());
        assert!(unknown_reason.is_err());
    }

    #[test]
    fn resolution_path_nodes_validate_public_coordinates() {
        let invalid_package = serde_json::from_str::<ResolutionPathNode>(
            r#"{"type":"package","ecosystem":"cargo","name":"example-runtime","version":"1.2"}"#,
        );
        let invalid_extension = serde_json::from_str::<ResolutionPathNode>(
            r#"{"type":"extension","extension_type":"skill","source":"symposium-recommendations","name":"private/skill"}"#,
        );

        assert!(invalid_package.is_err());
        assert!(invalid_extension.is_err());
    }
}
