//! Walking a site one level at a time.
//!
//! Every other page search descends a whole subtree, which is the wrong shape
//! for the thing an operator does first: open a section and see what is directly
//! inside it. Filtering a subtree search down to one level would traverse
//! everything below the anchor to discard almost all of it, and at a real site's
//! depth that is the difference between an answer and a budget refusal.
//!
//! The anchor is called `root_path` on purpose. It is the same anchor every
//! other rooted search takes, so a missing or inaccessible one produces the same
//! closed refusal with the same single field, and a caller who has learned that
//! refusal once has learned it here too.
//!
//! A match is a page and an immediate child. A grandchild does not match, and
//! neither does a child that is not a page - a `sling:Folder` under a site root
//! is a real thing to meet and is not a page.

use serde::de::Error as _;
use serde::{Deserialize, Serialize};

use crate::command::find_pages_containing_phrase::PageMatch;
use crate::command::query_paths::{
    DiscoveryResultFailure, anchor_contains, require_strictly_ascending,
};
use crate::command::repository_path::RepositoryPath;
use crate::command::result_window::{ContinuationToken, ResultWindow};

/// One request to list the pages directly below an anchor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListChildPagesCommand {
    /// Page the caller is asking for, when the caller said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_window: Option<ResultWindow>,
    /// Node whose immediate children are listed.
    pub root_path: RepositoryPath,
}

impl ListChildPagesCommand {
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

/// One page of pages directly below the anchor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ListChildPagesResult {
    /// Matches, strictly ascending by page path bytes.
    pub matches: Vec<PageMatch>,
    /// Where the next page resumes, when there is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_continuation_token: Option<ContinuationToken>,
}

impl ListChildPagesResult {
    /// Returns the page `matches` and `next_continuation_token` describe.
    ///
    /// # Errors
    ///
    /// Returns [`DiscoveryResultFailure::NotStrictlyAscending`] when a path
    /// repeats or sorts before its predecessor.
    pub fn new(
        matches: Vec<PageMatch>,
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
        command: &ListChildPagesCommand,
    ) -> Result<(), DiscoveryResultFailure> {
        let below = self.matches.iter().all(|found| {
            anchor_contains(&command.root_path, &found.repository_path)
                && command.is_immediate_child(&found.repository_path)
        });
        if below { Ok(()) } else { Err(DiscoveryResultFailure::NotThisRequest) }
    }
}

/// One page exactly as it is written on the wire.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ResultDocument {
    /// Matches this page carries.
    matches: Vec<PageMatch>,
    /// Where the next page resumes.
    #[serde(default)]
    next_continuation_token: Option<ContinuationToken>,
}

impl<'de> Deserialize<'de> for ListChildPagesResult {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        let document = ResultDocument::deserialize(deserializer)?;
        Self::new(document.matches, document.next_continuation_token).map_err(Source::Error::custom)
    }
}
