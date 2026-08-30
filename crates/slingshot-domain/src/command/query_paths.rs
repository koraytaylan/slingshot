//! Finding every node that answers a structured question.
//!
//! This is the general discovery command: a root to search under, an optional
//! primary type, and a bounded collection of predicates that all have to hold.
//! The four page and asset searches that follow are narrower questions with the
//! same shape.
//!
//! Two rules make an absent property behave predictably. A property that is not
//! there makes `Exists` false, and it makes every other operator false too -
//! including `NotEquals`, which reads as though absence ought to satisfy it.
//! The reason is that a discovery command answers about nodes it can see: a
//! node with no such property has not been shown to differ from the value, it
//! has been shown to have nothing to compare. Second, a discriminator or
//! cardinality mismatch is false rather than a coercion, so asking for the
//! integer one never matches the string one.
//!
//! Results are strictly ascending by repository path, with no path twice. That
//! ordering is what makes a continuation token meaningful: a page resumes after
//! its last path, and an unordered page would have no "after".

use serde::de::Error as _;
use serde::{Deserialize, Serialize};

use crate::command::repository_path::{PrimaryNodeTypeName, RepositoryPath};
use crate::command::result_window::{ContinuationToken, ResultWindow};
use crate::command::search_predicate::{PropertyPredicate, PropertyPredicates};

/// Failure naming an anchor that is not there.
pub const ROOT_NOT_FOUND_FAILURE: &str = "root_not_found";

/// Failure naming an anchor that is there and unreadable.
pub const ROOT_ACCESS_DENIED_FAILURE: &str = "root_access_denied";

/// Reason a discovery value could not be built.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DiscoveryResultFailure {
    /// Two matches carry the same path, or a later one sorts before an earlier.
    #[error("discovery matches are strictly ascending by repository path bytes")]
    NotStrictlyAscending,
    /// A result echoes an anchor its command did not ask about.
    #[error("a discovery result echoes the anchor its command asked about")]
    NotThisRequest,
}

/// Why a discovery command produced no page.
///
/// Both shapes are closed and neither carries matches or a continuation token:
/// an anchor that could not be resolved has no page to be partway through.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "failure", rename_all = "snake_case", deny_unknown_fields)]
pub enum AnchorRefusal {
    /// The anchor is not there.
    RootNotFound {
        /// Anchor that is not there.
        root_path: RepositoryPath,
    },
    /// The anchor is there and unreadable.
    RootAccessDenied {
        /// Anchor that could not be read.
        root_path: RepositoryPath,
    },
}

impl AnchorRefusal {
    /// Returns the anchor this refusal names.
    #[must_use]
    pub fn root_path(&self) -> &RepositoryPath {
        match self {
            Self::RootNotFound { root_path } | Self::RootAccessDenied { root_path } => root_path,
        }
    }

    /// Requires this refusal to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`DiscoveryResultFailure::NotThisRequest`] when the echoed
    /// anchor is another request's, which is the only thing distinguishing two
    /// otherwise identical refusals.
    pub fn require_answers(
        &self,
        command: &QueryPathsCommand,
    ) -> Result<(), DiscoveryResultFailure> {
        if *self.root_path() == command.root_path {
            Ok(())
        } else {
            Err(DiscoveryResultFailure::NotThisRequest)
        }
    }
}

/// Returns whether `anchor` is at or above `path`.
///
/// Byte containment on segment boundaries, so `/content/ex` does not contain
/// `/content/example` the way a plain prefix test would say it does. Every
/// discovery command correlates its matches with its anchor this way, so the
/// rule lives here rather than five more times.
#[must_use]
pub fn anchor_contains(anchor: &RepositoryPath, path: &RepositoryPath) -> bool {
    if anchor.is_root() {
        return true;
    }
    let anchor = anchor.as_text();
    let path = path.as_text();
    anchor == path || path.strip_prefix(anchor).is_some_and(|rest| rest.starts_with('/'))
}

/// One request to find nodes under an anchor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueryPathsCommand {
    /// Exact primary type a node must have, when the caller named one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_node_type: Option<PrimaryNodeTypeName>,
    /// Questions every match must answer, combined with logical and.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub property_predicates: Option<PropertyPredicates>,
    /// Page the caller is asking for, when the caller said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_window: Option<ResultWindow>,
    /// Node to search at and below.
    pub root_path: RepositoryPath,
}

impl QueryPathsCommand {
    /// Returns the questions every match must answer.
    ///
    /// A search that names no property predicate is constrained by its anchor
    /// and its primary type alone, which is a legal question rather than an
    /// omission to complain about.
    #[must_use]
    pub fn predicates(&self) -> &[PropertyPredicate] {
        self.property_predicates.as_ref().map_or(&[], PropertyPredicates::predicates)
    }

    /// Returns the page this request asks for, stated or resolved.
    #[must_use]
    pub fn resolved_window(&self) -> ResultWindow {
        self.result_window.clone().unwrap_or_default()
    }
}

/// One node that answered the question.
///
/// A match is its path and nothing else. A general query cannot know which
/// properties the caller wanted back, and returning the ones it happened to
/// evaluate would make the result depend on the order the predicates ran in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PathMatch {
    /// Node that matched.
    pub repository_path: RepositoryPath,
}

/// One page of nodes that answered the question.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct QueryPathsResult {
    /// Matches, strictly ascending by repository path bytes.
    pub matches: Vec<PathMatch>,
    /// Where the next page resumes, when there is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_continuation_token: Option<ContinuationToken>,
}

impl QueryPathsResult {
    /// Returns the page `matches` and `next_continuation_token` describe.
    ///
    /// # Errors
    ///
    /// Returns [`DiscoveryResultFailure::NotStrictlyAscending`] when a path
    /// repeats or sorts before its predecessor.
    pub fn new(
        matches: Vec<PathMatch>,
        next_continuation_token: Option<ContinuationToken>,
    ) -> Result<Self, DiscoveryResultFailure> {
        require_strictly_ascending(matches.iter().map(|found| &found.repository_path))?;
        Ok(Self { matches, next_continuation_token })
    }

    /// Requires this page to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`DiscoveryResultFailure::NotThisRequest`] when a match lies
    /// outside the anchor the command asked about.
    pub fn require_answers(
        &self,
        command: &QueryPathsCommand,
    ) -> Result<(), DiscoveryResultFailure> {
        let within = self
            .matches
            .iter()
            .all(|found| anchor_contains(&command.root_path, &found.repository_path));
        if within { Ok(()) } else { Err(DiscoveryResultFailure::NotThisRequest) }
    }
}

/// Requires a sequence of paths to be strictly ascending by bytes.
///
/// Strictly, so a repeated path is refused rather than deduplicated: two
/// matches at one address mean the enumeration visited something twice, and
/// quietly collapsing them would hide that.
///
/// # Errors
///
/// Returns [`DiscoveryResultFailure::NotStrictlyAscending`] at the first pair
/// that is not.
pub fn require_strictly_ascending<'path>(
    paths: impl IntoIterator<Item = &'path RepositoryPath>,
) -> Result<(), DiscoveryResultFailure> {
    let mut previous: Option<&RepositoryPath> = None;
    for path in paths {
        if let Some(earlier) = previous
            && earlier.as_text().as_bytes() >= path.as_text().as_bytes()
        {
            return Err(DiscoveryResultFailure::NotStrictlyAscending);
        }
        previous = Some(path);
    }
    Ok(())
}

/// One page exactly as it is written on the wire.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ResultDocument {
    /// Matches this page carries.
    matches: Vec<PathMatch>,
    /// Where the next page resumes.
    #[serde(default)]
    next_continuation_token: Option<ContinuationToken>,
}

impl<'de> Deserialize<'de> for QueryPathsResult {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        let document = ResultDocument::deserialize(deserializer)?;
        Self::new(document.matches, document.next_continuation_token).map_err(Source::Error::custom)
    }
}
