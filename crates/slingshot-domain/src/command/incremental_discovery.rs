//! Versioned live traversal pages, whose order is supplied by the repository provider.

use std::collections::BTreeSet;

use serde::Serialize;
use serde_json::{Value, json};

use crate::command::command_identity::CommandContract;
use crate::command::repository_path::RepositoryPath;
use crate::command::result_window::ContinuationToken;
use crate::command::schema::nonempty_string;

/// Explicit progress for a single bounded traversal page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct DiscoveryProgress {
    /// Whether all retained iterators were exhausted, rather than a per-page budget.
    pub complete: bool,
    /// Nodes examined during this page, including nodes that did not produce a match.
    pub examined_nodes: u64,
}

/// A malformed incremental page that cannot safely be presented as discovery progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum IncrementalDiscoveryFailure {
    /// A partial page lacks a token, or a complete page claims a successor.
    #[error("a partial discovery page has a continuation and a complete page does not")]
    InconsistentCompletion,
    /// The page reports more matches than examined nodes or exceeds a declared bound.
    #[error("discovery progress and match counts must fit the declared per-page bounds")]
    InvalidCounts,
    /// Provider order may vary, but a path cannot occur twice in one page.
    #[error("an incremental discovery page cannot repeat a repository path")]
    RepeatedPath,
}

/// Validates explicit progress and unique paths without imposing a global lexical order.
///
/// # Errors
///
/// Returns an inconsistency for mismatched completeness, excessive counts, or a repeated path.
pub fn require_page<'paths>(
    paths: impl IntoIterator<Item = &'paths RepositoryPath>,
    next: Option<&ContinuationToken>,
    progress: DiscoveryProgress,
) -> Result<(), IncrementalDiscoveryFailure> {
    if next.is_some() == progress.complete {
        return Err(IncrementalDiscoveryFailure::InconsistentCompletion);
    }
    let mut seen = BTreeSet::new();
    for path in paths {
        if !seen.insert(path) {
            return Err(IncrementalDiscoveryFailure::RepeatedPath);
        }
    }
    let count = u64::try_from(seen.len()).unwrap_or(u64::MAX);
    let contract = CommandContract::embedded();
    if count > progress.examined_nodes
        || count > contract.limit("maximum_result_limit")
        || progress.examined_nodes > contract.limit("maximum_discovery_candidate_nodes")
    {
        return Err(IncrementalDiscoveryFailure::InvalidCounts);
    }
    Ok(())
}

/// Returns the closed result schema for the next discovery command version.
#[must_use]
pub(crate) fn page_schema(limits: &CommandContract, row: Value) -> Value {
    let properties = json!({
        "complete": {"const": true},
        "examined_nodes": {"type": "integer", "minimum": 0,
            "maximum": limits.limit("maximum_discovery_candidate_nodes")},
        "matches": {"type": "array", "maxItems": limits.limit("maximum_result_limit"), "items": row},
    });
    let complete = json!({"type": "object", "additionalProperties": false,
        "required": ["complete", "examined_nodes", "matches"], "properties": properties});
    let mut partial = complete.clone();
    partial["properties"]["complete"] = json!({"const": false});
    partial["properties"]["next_continuation_token"] =
        nonempty_string(limits.limit("maximum_continuation_token_bytes"));
    partial["required"] =
        json!(["complete", "examined_nodes", "matches", "next_continuation_token"]);
    json!({"type": "object", "oneOf": [complete, partial]})
}
