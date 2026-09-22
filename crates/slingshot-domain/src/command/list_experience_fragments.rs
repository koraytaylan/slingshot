//! The experience fragments under one anchor.
//!
//! An experience fragment is the page whose content resource type is the
//! experience-fragment page. The variations underneath it are pages too, and
//! they are how the fragment renders, so listing them would hand
//! `delete_experience_fragment` an address that is a variation rather than the
//! fragment. This command lists the fragment.

use serde::de::Error as _;
use serde::{Deserialize, Serialize};

use crate::command::find_pages_containing_phrase::PageMatch;
use crate::command::query_paths::{
    DiscoveryResultFailure, anchor_contains, require_strictly_ascending,
};
use crate::command::repository_path::RepositoryPath;
use crate::command::result_window::{ContinuationToken, ResultWindow};

/// One request to list the experience fragments under an anchor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListExperienceFragmentsCommand {
    /// Page the caller is asking for, when the caller said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_window: Option<ResultWindow>,
    /// Node to search under.
    pub root_path: RepositoryPath,
}

impl ListExperienceFragmentsCommand {
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

/// One page of experience fragments under the anchor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ListExperienceFragmentsResult {
    /// Matches, strictly ascending by fragment path bytes.
    pub matches: Vec<PageMatch>,
    /// Where the next page resumes, when there is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_continuation_token: Option<ContinuationToken>,
}

impl ListExperienceFragmentsResult {
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
    /// Returns [`DiscoveryResultFailure::NotThisRequest`] when a match is not
    /// under the anchor the command asked about.
    pub fn require_answers(
        &self,
        command: &ListExperienceFragmentsCommand,
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

impl<'de> Deserialize<'de> for ListExperienceFragmentsResult {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        let document = ResultDocument::deserialize(deserializer)?;
        Self::new(document.matches, document.next_continuation_token).map_err(Source::Error::custom)
    }
}
