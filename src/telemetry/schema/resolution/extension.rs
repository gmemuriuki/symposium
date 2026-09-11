//! Safe extension-resolution path vocabulary.

use std::fmt;

use serde::{Deserialize, Serialize};

use super::{
    super::extension::{
        ExtensionKind, PublicExtensionCoordinate, PublicExtensionName, PublicExtensionSource,
    },
    package::PublicPackageCoordinate,
};

/// Maximum root-to-leaf depth of a recorded resolution path.
const MAX_RESOLUTION_PATH_DEPTH: usize = 8;

/// Maximum combined terminal-node count in a recorded resolution path.
const MAX_RESOLUTION_PATH_LEAVES: usize = 16;

/// Maximum compact UTF-8 JSON size of a complete resolution path.
const MAX_RESOLUTION_PATH_ENCODED_BYTES: usize = 4 * 1024;

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

impl ResolutionPathNode {
    fn validate_depth(&self, depth: usize) -> Result<(), ResolutionPathError> {
        if depth > MAX_RESOLUTION_PATH_DEPTH {
            return Err(ResolutionPathError::DepthExceeded {
                observed: depth,
                maximum: MAX_RESOLUTION_PATH_DEPTH,
            });
        }

        match self {
            Self::All(node) => {
                let child_depth = depth
                    .checked_add(1)
                    .expect("BUG: resolution path depth is bounded before descending");
                for child in &node.children {
                    child.validate_depth(child_depth)?;
                }
            }
            Self::Any(node) => {
                let child_depth = depth
                    .checked_add(1)
                    .expect("BUG: resolution path depth is bounded before descending");
                node.child.validate_depth(child_depth)?;
            }
            Self::Package(_) | Self::Extension(_) | Self::Not(_) | Self::Opaque(_) => {}
        }

        Ok(())
    }

    fn count_leaves(&self, leaf_count: &mut usize) -> Result<(), ResolutionPathError> {
        match self {
            Self::All(node) => {
                for child in &node.children {
                    child.count_leaves(leaf_count)?;
                }
            }
            Self::Any(node) => node.child.count_leaves(leaf_count)?,
            Self::Package(_) | Self::Extension(_) | Self::Not(_) | Self::Opaque(_) => {
                let observed = leaf_count
                    .checked_add(1)
                    .expect("BUG: resolution path leaf count is bounded before incrementing");

                if observed > MAX_RESOLUTION_PATH_LEAVES {
                    return Err(ResolutionPathError::LeafCountExceeded {
                        observed,
                        maximum: MAX_RESOLUTION_PATH_LEAVES,
                    });
                }

                *leaf_count = observed;
            }
        }

        Ok(())
    }
}

/// Complete evidence path for one successful extension resolution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "Vec<ResolutionPathNode>")]
struct ResolutionPath(Vec<ResolutionPathNode>);

impl TryFrom<Vec<ResolutionPathNode>> for ResolutionPath {
    type Error = ResolutionPathError;

    fn try_from(nodes: Vec<ResolutionPathNode>) -> Result<Self, Self::Error> {
        if nodes.is_empty() {
            return Err(ResolutionPathError::Empty);
        }

        for node in &nodes {
            node.validate_depth(1)?;
        }

        let mut leaf_count = 0;
        for node in &nodes {
            node.count_leaves(&mut leaf_count)?;
        }

        let encoded_size = serde_json::to_vec(&nodes)
            .expect("BUG: resolution path nodes must have an infallible JSON representation")
            .len();
        if encoded_size > MAX_RESOLUTION_PATH_ENCODED_BYTES {
            return Err(ResolutionPathError::EncodedSizeExceeded {
                observed: encoded_size,
                maximum: MAX_RESOLUTION_PATH_ENCODED_BYTES,
            });
        }

        Ok(Self(nodes))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResolutionPathError {
    Empty,
    DepthExceeded { observed: usize, maximum: usize },
    LeafCountExceeded { observed: usize, maximum: usize },
    EncodedSizeExceeded { observed: usize, maximum: usize },
}

impl fmt::Display for ResolutionPathError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("resolution path must contain at least one node"),
            Self::DepthExceeded { observed, maximum } => write!(
                formatter,
                "resolution path depth {observed} exceeds maximum {maximum}"
            ),
            Self::LeafCountExceeded { observed, maximum } => write!(
                formatter,
                "resolution path leaf count {observed} exceeds maximum {maximum}"
            ),
            Self::EncodedSizeExceeded { observed, maximum } => write!(
                formatter,
                "resolution path encoded size {observed} bytes exceeds maximum {maximum} bytes"
            ),
        }
    }
}

impl std::error::Error for ResolutionPathError {}

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

    fn resolution_path_with_depth(depth: usize) -> String {
        assert!(depth > 0);

        let mut node = r#"{"type":"not"}"#.to_owned();
        for _ in 1..depth {
            node = format!(r#"{{"type":"any","child":{node}}}"#);
        }

        format!("[{node}]")
    }

    fn resolution_path_with_all_depth(depth: usize) -> String {
        assert!(depth > 0);

        let mut node = r#"{"type":"not"}"#.to_owned();
        for _ in 1..depth {
            node = format!(r#"{{"type":"all","children":[{node}]}}"#);
        }

        format!("[{node}]")
    }

    fn resolution_path_with_leaves(leaves: usize) -> String {
        let nodes = std::iter::repeat_n(r#"{"type":"not"}"#, leaves)
            .collect::<Vec<_>>()
            .join(",");

        format!("[{nodes}]")
    }

    fn package_resolution_path_with_encoded_size(encoded_size: usize) -> String {
        const PREFIX: &str =
            r#"[{"type":"package","ecosystem":"cargo","name":"a","version":"1.2.3+"#;
        const SUFFIX: &str = r#""}]"#;

        let metadata_size = encoded_size
            .checked_sub(PREFIX.len() + SUFFIX.len())
            .unwrap();
        let json = format!("{PREFIX}{}{SUFFIX}", "a".repeat(metadata_size));

        assert_eq!(json.len(), encoded_size);
        json
    }

    fn resolution_path_with_leaves_nested_under_all_and_any(
        left_leaves: usize,
        right_leaves: usize,
    ) -> String {
        let left_children = std::iter::repeat_n(serde_json::json!({ "type": "not" }), left_leaves)
            .collect::<Vec<_>>();
        let right_children =
            std::iter::repeat_n(serde_json::json!({ "type": "not" }), right_leaves)
                .collect::<Vec<_>>();

        serde_json::json!([{
            "type": "all",
            "children": [
                { "type": "all", "children": left_children },
                {
                    "type": "any",
                    "child": { "type": "all", "children": right_children }
                }
            ]
        }])
        .to_string()
    }

    fn validate_resolution_path(json: &str) -> Result<ResolutionPath, ResolutionPathError> {
        let nodes = serde_json::from_str::<Vec<ResolutionPathNode>>(json)
            .expect("BUG: generated test path must contain valid resolution nodes");

        ResolutionPath::try_from(nodes)
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
    fn non_empty_resolution_path_round_trips_as_an_array() {
        let json = r#"[{"type":"not"}]"#;

        let path = serde_json::from_str::<ResolutionPath>(json).unwrap();
        let encoded = serde_json::to_string(&path).unwrap();

        assert_eq!(encoded, json);
    }

    #[test]
    fn empty_resolution_path_is_rejected_at_both_boundaries() {
        let constructed = ResolutionPath::try_from(Vec::new());
        let deserialized = serde_json::from_str::<ResolutionPath>("[]");

        assert_eq!(constructed.unwrap_err(), ResolutionPathError::Empty);
        assert!(
            deserialized
                .unwrap_err()
                .to_string()
                .contains("resolution path must contain at least one node")
        );
    }

    #[test]
    fn resolution_path_accepts_depth_eight_and_rejects_depth_nine() {
        let at_limit = resolution_path_with_depth(8);
        let beyond_limit = resolution_path_with_depth(9);

        let accepted = validate_resolution_path(&at_limit);
        let rejected = validate_resolution_path(&beyond_limit);

        assert!(accepted.is_ok());
        assert_eq!(
            rejected.unwrap_err(),
            ResolutionPathError::DepthExceeded {
                observed: 9,
                maximum: 8,
            }
        );
    }

    #[test]
    fn resolution_path_counts_depth_through_all_nodes() {
        let at_limit = resolution_path_with_all_depth(8);
        let beyond_limit = resolution_path_with_all_depth(9);

        let accepted = validate_resolution_path(&at_limit);
        let rejected = validate_resolution_path(&beyond_limit);

        assert!(accepted.is_ok());
        assert_eq!(
            rejected.unwrap_err(),
            ResolutionPathError::DepthExceeded {
                observed: 9,
                maximum: 8,
            }
        );
    }

    #[test]
    fn resolution_path_accepts_sixteen_leaves_and_rejects_seventeen() {
        let at_limit = resolution_path_with_leaves(16);
        let beyond_limit = resolution_path_with_leaves(17);

        let accepted = validate_resolution_path(&at_limit);
        let rejected = validate_resolution_path(&beyond_limit);

        assert!(accepted.is_ok());
        assert_eq!(
            rejected.unwrap_err(),
            ResolutionPathError::LeafCountExceeded {
                observed: 17,
                maximum: 16,
            }
        );
    }

    #[test]
    fn resolution_path_counts_only_terminal_leaves_across_nested_all_and_any_nodes() {
        let at_limit = resolution_path_with_leaves_nested_under_all_and_any(8, 8);
        let beyond_limit = resolution_path_with_leaves_nested_under_all_and_any(8, 9);

        let accepted = validate_resolution_path(&at_limit);
        let rejected = validate_resolution_path(&beyond_limit);

        assert!(accepted.is_ok());
        assert_eq!(
            rejected.unwrap_err(),
            ResolutionPathError::LeafCountExceeded {
                observed: 17,
                maximum: 16,
            }
        );
    }

    #[test]
    fn resolution_path_accepts_4096_bytes_and_rejects_4097() {
        let at_limit = package_resolution_path_with_encoded_size(4_096);
        let beyond_limit = package_resolution_path_with_encoded_size(4_097);

        let accepted = validate_resolution_path(&at_limit);
        let rejected = validate_resolution_path(&beyond_limit);

        assert!(accepted.is_ok());
        assert_eq!(
            rejected.unwrap_err(),
            ResolutionPathError::EncodedSizeExceeded {
                observed: 4_097,
                maximum: 4_096,
            }
        );
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
