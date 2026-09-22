//! The editable page templates under one anchor.
//!
//! Creating a page needs a template path, and the path is not something an
//! operator remembers: editable templates live under `settings/wcm/templates`
//! inside a configuration, and the name a person knows is the title. This is
//! the command `create_page` is reached through.
//!
//! A match is the template node itself. The structure, the policies, and the
//! initial content underneath it are parts of that template, and listing them
//! would hand `create_page` an address it would refuse as not a template.

use serde::de::Error as _;
use serde::{Deserialize, Serialize};

use crate::command::find_pages_containing_phrase::PageMatch;
use crate::command::query_paths::{
    DiscoveryResultFailure, anchor_contains, direct_child_of, require_strictly_ascending,
};
use crate::command::repository_path::RepositoryPath;
use crate::command::result_window::{ContinuationToken, ResultWindow};

/// Segments immediately above an editable page template.
const PAGE_TEMPLATE_PARENTS: &[&str] = &["settings", "wcm", "templates"];

/// One request to list the editable page templates under an anchor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListPageTemplatesCommand {
    /// Page the caller is asking for, when the caller said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_window: Option<ResultWindow>,
    /// Node to search under.
    pub root_path: RepositoryPath,
}

impl ListPageTemplatesCommand {
    /// Returns the page this request asks for, stated or resolved.
    #[must_use]
    pub fn resolved_window(&self) -> ResultWindow {
        self.result_window.clone().unwrap_or_default()
    }

    /// Returns whether `candidate` is an editable page template under this anchor.
    #[must_use]
    pub fn admits(&self, candidate: &RepositoryPath) -> bool {
        anchor_contains(&self.root_path, candidate)
            && *candidate != self.root_path
            && direct_child_of(candidate, PAGE_TEMPLATE_PARENTS)
    }
}

/// One page of editable page templates under the anchor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ListPageTemplatesResult {
    /// Matches, strictly ascending by template path bytes.
    pub matches: Vec<PageMatch>,
    /// Where the next page resumes, when there is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_continuation_token: Option<ContinuationToken>,
}

impl ListPageTemplatesResult {
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
    /// editable page template under the anchor the command asked about.
    pub fn require_answers(
        &self,
        command: &ListPageTemplatesCommand,
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
}

impl<'de> Deserialize<'de> for ListPageTemplatesResult {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        let document = ResultDocument::deserialize(deserializer)?;
        Self::new(document.matches, document.next_continuation_token).map_err(Source::Error::custom)
    }
}
