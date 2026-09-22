//! Walking a site one level at a time, whatever the children are.
//!
//! Listing the pages directly below an anchor answers the common case, and it
//! answers only that case: a folder beside those pages, a fragment, an asset,
//! and a component are all children an operator needs to see, and a listing
//! that silently drops them is a listing that lies about the shape of a tree.
//!
//! So the general read is here, and the page listing is a specialization of it.
//! [`ListChildNodesCommand`] lists every immediate child; and
//! [`ListChildNodesByTypeCommand`] lists the immediate children that are exactly
//! one named primary type. A page listing is the second with `cq:Page` as the
//! type, which is why the two commands agree on every rule but the filter: the
//! anchor is the same anchor, a grandchild is not a child, a non-matching child
//! does not match, and the order is the repository path's ascending bytes.
//!
//! The anchor is called `root_path` on purpose. It is the same anchor every
//! other rooted search takes, so a missing or inaccessible one produces the same
//! closed refusal with the same single field, and a caller who has learned that
//! refusal once has learned it here too.

use serde::de::Error as _;
use serde::{Deserialize, Serialize};

use crate::command::find_pages_containing_phrase::{PAGE_PRIMARY_NODE_TYPE, PageTitle};
use crate::command::query_paths::{
    DiscoveryResultFailure, anchor_contains, require_strictly_ascending,
};
use crate::command::repository_path::{PrimaryNodeTypeName, RepositoryPath};
use crate::command::result_window::{ContinuationToken, ResultWindow};

/// Returns the primary type a page listing filters on.
///
/// The page listing is this command's own type filter with the type a page has,
/// so the literal lives here once and the two can never disagree.
#[must_use]
pub fn page_primary_node_type() -> PrimaryNodeTypeName {
    PrimaryNodeTypeName::parse(PAGE_PRIMARY_NODE_TYPE)
        .expect("the page primary type is a qualified repository name")
}

/// One request to list the immediate children of an anchor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListChildNodesCommand {
    /// Page the caller is asking for, when the caller said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_window: Option<ResultWindow>,
    /// Node whose immediate children are listed.
    pub root_path: RepositoryPath,
}

impl ListChildNodesCommand {
    /// Returns the page this request asks for, stated or resolved.
    #[must_use]
    pub fn resolved_window(&self) -> ResultWindow {
        self.result_window.clone().unwrap_or_default()
    }

    /// Returns whether `candidate` is directly below this anchor.
    ///
    /// Parent equality rather than prefix arithmetic, so a grandchild is not a
    /// child and a sibling whose name begins with the anchor's is not either.
    #[must_use]
    pub fn is_immediate_child(&self, candidate: &RepositoryPath) -> bool {
        candidate.parent().is_some_and(|parent| parent == self.root_path)
    }
}

/// One request to list the immediate children of an anchor that are one type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListChildNodesByTypeCommand {
    /// Exact primary type a child must have to match.
    pub primary_node_type: PrimaryNodeTypeName,
    /// Page the caller is asking for, when the caller said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_window: Option<ResultWindow>,
    /// Node whose immediate children are listed.
    pub root_path: RepositoryPath,
}

impl ListChildNodesByTypeCommand {
    /// Returns the page this request asks for, stated or resolved.
    #[must_use]
    pub fn resolved_window(&self) -> ResultWindow {
        self.result_window.clone().unwrap_or_default()
    }

    /// Returns whether `candidate` is directly below this anchor.
    #[must_use]
    pub fn is_immediate_child(&self, candidate: &RepositoryPath) -> bool {
        candidate.parent().is_some_and(|parent| parent == self.root_path)
    }
}

/// One child node that matched a listing.
///
/// The path names the node itself, the primary type names what it is - which is
/// what distinguishes a page from a folder beside it - and the title is optional
/// because a node need not have one. A title comes only from a single-valued
/// `jcr:title` string on the node's content resource, and only where the node
/// has one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChildNodeMatch {
    /// Exact primary type the child has.
    pub primary_node_type: PrimaryNodeTypeName,
    /// Node that matched.
    pub repository_path: RepositoryPath,
    /// Its title, when it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<PageTitle>,
}

/// Returns whether every match is an immediate child of `anchor`.
fn every_immediate_child<'found>(
    anchor: &RepositoryPath,
    matches: impl IntoIterator<Item = &'found ChildNodeMatch>,
) -> bool {
    matches.into_iter().all(|found| {
        anchor_contains(anchor, &found.repository_path)
            && found.repository_path.parent().is_some_and(|parent| parent == *anchor)
    })
}

/// One page of children directly below the anchor.
///
/// A typed listing answers the same document - the children one type among the
/// immediate children of one anchor - so the two result types are separate only
/// because each answers its own request, and agree on every member.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ListChildNodesResult {
    /// Matches, strictly ascending by node path bytes.
    pub matches: Vec<ChildNodeMatch>,
    /// Where the next page resumes, when there is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_continuation_token: Option<ContinuationToken>,
}

impl ListChildNodesResult {
    /// Returns the page `matches` and `next_continuation_token` describe.
    ///
    /// # Errors
    ///
    /// Returns [`DiscoveryResultFailure::NotStrictlyAscending`] when a path
    /// repeats or sorts before its predecessor.
    pub fn new(
        matches: Vec<ChildNodeMatch>,
        next_continuation_token: Option<ContinuationToken>,
    ) -> Result<Self, DiscoveryResultFailure> {
        require_strictly_ascending(matches.iter().map(|found| &found.repository_path))?;
        Ok(Self { matches, next_continuation_token })
    }

    /// Requires this page to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`DiscoveryResultFailure::NotThisRequest`] when a match is not an
    /// immediate child of the anchor the command asked about.
    pub fn require_answers(
        &self,
        command: &ListChildNodesCommand,
    ) -> Result<(), DiscoveryResultFailure> {
        if every_immediate_child(&command.root_path, &self.matches) {
            Ok(())
        } else {
            Err(DiscoveryResultFailure::NotThisRequest)
        }
    }
}

/// One page of children of one primary type directly below the anchor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ListChildNodesByTypeResult {
    /// Matches, strictly ascending by node path bytes.
    pub matches: Vec<ChildNodeMatch>,
    /// Where the next page resumes, when there is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_continuation_token: Option<ContinuationToken>,
}

impl ListChildNodesByTypeResult {
    /// Returns the page `matches` and `next_continuation_token` describe.
    ///
    /// # Errors
    ///
    /// Returns [`DiscoveryResultFailure::NotStrictlyAscending`] when a path
    /// repeats or sorts before its predecessor.
    pub fn new(
        matches: Vec<ChildNodeMatch>,
        next_continuation_token: Option<ContinuationToken>,
    ) -> Result<Self, DiscoveryResultFailure> {
        require_strictly_ascending(matches.iter().map(|found| &found.repository_path))?;
        Ok(Self { matches, next_continuation_token })
    }

    /// Requires this page to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`DiscoveryResultFailure::NotThisRequest`] when a match is not an
    /// immediate child of the anchor, or does not have the primary type, the
    /// command asked about.
    pub fn require_answers(
        &self,
        command: &ListChildNodesByTypeCommand,
    ) -> Result<(), DiscoveryResultFailure> {
        let below = every_immediate_child(&command.root_path, &self.matches)
            && self
                .matches
                .iter()
                .all(|found| found.primary_node_type == command.primary_node_type);
        if below { Ok(()) } else { Err(DiscoveryResultFailure::NotThisRequest) }
    }
}

/// One page exactly as it is written on the wire.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ResultDocument {
    /// Matches this page carries.
    matches: Vec<ChildNodeMatch>,
    /// Where the next page resumes.
    #[serde(default)]
    next_continuation_token: Option<ContinuationToken>,
}

impl<'de> Deserialize<'de> for ListChildNodesResult {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        let document = ResultDocument::deserialize(deserializer)?;
        Self::new(document.matches, document.next_continuation_token).map_err(Source::Error::custom)
    }
}

impl<'de> Deserialize<'de> for ListChildNodesByTypeResult {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        let document = ResultDocument::deserialize(deserializer)?;
        Self::new(document.matches, document.next_continuation_token).map_err(Source::Error::custom)
    }
}
