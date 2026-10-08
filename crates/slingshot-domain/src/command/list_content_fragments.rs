//! The content fragments under one anchor.
//!
//! A content fragment is a `dam:Asset` whose content node carries the fragment
//! flag. An image in the same folder is an asset and not a fragment, which is
//! why this is not a listing of assets. The match is the fragment itself:
//! `read_content_fragment` and `delete_content_fragment` take that path, and a
//! variation underneath it is part of the fragment rather than another one.

use serde::de::Error as _;
use serde::{Deserialize, Serialize};

use crate::command::find_pages_containing_phrase::PageMatch;
use crate::command::incremental_discovery::{
    DiscoveryProgress, IncrementalDiscoveryFailure, require_page,
};
use crate::command::query_paths::{DiscoveryResultFailure, anchor_contains};
use crate::command::repository_path::RepositoryPath;
use crate::command::result_window::{ContinuationToken, ResultWindow};

/// One request to list the content fragments under an anchor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListContentFragmentsCommand {
    /// Page the caller is asking for, when the caller said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_window: Option<ResultWindow>,
    /// Node to search under.
    pub root_path: RepositoryPath,
}

impl ListContentFragmentsCommand {
    /// Returns the page this request asks for, stated or resolved.
    #[must_use]
    pub fn resolved_window(&self) -> ResultWindow {
        self.result_window.clone().unwrap_or_default()
    }

    /// Returns whether `candidate` sits strictly under this anchor.
    #[must_use]
    pub fn admits(&self, candidate: &RepositoryPath) -> bool {
        anchor_contains(&self.root_path, candidate) && *candidate != self.root_path
    }
}

/// One page of content fragments under the anchor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ListContentFragmentsResult {
    /// Matches in live repository provider order, without repeated paths.
    pub matches: Vec<PageMatch>,
    /// Where the next page resumes, when there is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_continuation_token: Option<ContinuationToken>,
    /// Explicit completeness and the work performed during this page.
    #[serde(flatten)]
    pub progress: DiscoveryProgress,
}

impl ListContentFragmentsResult {
    /// Returns the page `matches` and `next_continuation_token` describe.
    ///
    /// # Errors
    ///
    /// Returns an incremental discovery failure when completeness is inconsistent,
    /// counts exceed their bounds, or one page repeats a path.
    pub fn new(
        matches: Vec<PageMatch>,
        next_continuation_token: Option<ContinuationToken>,
        progress: DiscoveryProgress,
    ) -> Result<Self, IncrementalDiscoveryFailure> {
        require_page(
            matches.iter().map(|found| &found.repository_path),
            next_continuation_token.as_ref(),
            progress,
        )?;
        Ok(Self { matches, next_continuation_token, progress })
    }

    /// Requires this page to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`DiscoveryResultFailure::NotThisRequest`] when a match is not
    /// under the anchor the command asked about.
    pub fn require_answers(
        &self,
        command: &ListContentFragmentsCommand,
    ) -> Result<(), DiscoveryResultFailure> {
        if self.matches.iter().all(|found| command.admits(&found.repository_path)) {
            Ok(())
        } else {
            Err(DiscoveryResultFailure::NotThisRequest)
        }
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
    /// Whether the retained traversal ended.
    complete: bool,
    /// Nodes examined while producing this page.
    examined_nodes: u64,
}

impl<'de> Deserialize<'de> for ListContentFragmentsResult {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        let document = ResultDocument::deserialize(deserializer)?;
        Self::new(
            document.matches,
            document.next_continuation_token,
            DiscoveryProgress {
                complete: document.complete,
                examined_nodes: document.examined_nodes,
            },
        )
        .map_err(Source::Error::custom)
    }
}
