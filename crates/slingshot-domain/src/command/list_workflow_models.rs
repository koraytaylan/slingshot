//! Which workflows a deployment has.
//!
//! Starting a workflow needs a model identifier, and an operator has no way to
//! learn one: models live in different places on different versions, and the
//! identifier is not the title anybody knows the workflow by. This is the
//! command every other workflow command is reached through.
//!
//! The title prefix filters on the title rather than on the identifier, because
//! the title is what a person recognizes and the identifier is what a machine
//! needs. Filtering on the thing the caller already has would be no help.

use serde::de::Error as _;
use serde::{Deserialize, Serialize};

use crate::command::find_pages_containing_phrase::PageTitle;
use crate::command::operational_listing::{ListingResultFailure, require_strictly_ascending_text};
use crate::command::process_identity::WorkflowModelIdentifier;
use crate::command::result_window::{ContinuationToken, ResultWindow};

/// One request to list workflow models.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListWorkflowModelsCommand {
    /// Page the caller is asking for, when the caller said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_window: Option<ResultWindow>,
    /// Prefix every reported title begins with, when the caller said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title_prefix: Option<PageTitle>,
}

impl ListWorkflowModelsCommand {
    /// Returns the page this request asks for, stated or resolved.
    #[must_use]
    pub fn resolved_window(&self) -> ResultWindow {
        self.result_window.clone().unwrap_or_default()
    }

    /// Returns whether a model so titled is one this request asked about.
    #[must_use]
    pub fn admits(&self, title: &PageTitle) -> bool {
        self.title_prefix
            .as_ref()
            .is_none_or(|prefix| title.as_text().starts_with(prefix.as_text()))
    }
}

/// One workflow model the author reports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowModelMatch {
    /// The identifier the model is started by.
    pub model_identifier: WorkflowModelIdentifier,
    /// The title a person recognizes it by.
    pub title: PageTitle,
    /// The version the author reports, when it reports one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// One page of workflow models.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ListWorkflowModelsResult {
    /// Matches, strictly ascending by model identifier bytes.
    pub matches: Vec<WorkflowModelMatch>,
    /// Where the next page resumes, when there is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_continuation_token: Option<ContinuationToken>,
}

impl ListWorkflowModelsResult {
    /// Returns the page these matches describe.
    ///
    /// # Errors
    ///
    /// Returns [`ListingResultFailure::NotStrictlyAscending`] when an identifier
    /// repeats or sorts before its predecessor.
    pub fn new(
        matches: Vec<WorkflowModelMatch>,
        next_continuation_token: Option<ContinuationToken>,
    ) -> Result<Self, ListingResultFailure> {
        require_strictly_ascending_text(
            matches.iter().map(|found| found.model_identifier.as_text()),
        )?;
        Ok(Self { matches, next_continuation_token })
    }

    /// Requires this page to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`ListingResultFailure::NotThisRequest`] when a match's title
    /// does not carry the prefix the command asked about.
    pub fn require_answers(
        &self,
        command: &ListWorkflowModelsCommand,
    ) -> Result<(), ListingResultFailure> {
        let admitted = self.matches.iter().all(|found| command.admits(&found.title));
        if admitted { Ok(()) } else { Err(ListingResultFailure::NotThisRequest) }
    }
}

/// One page exactly as it is written on the wire.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ResultDocument {
    /// Matches this page carries.
    matches: Vec<WorkflowModelMatch>,
    /// Where the next page resumes.
    #[serde(default)]
    next_continuation_token: Option<ContinuationToken>,
}

impl<'de> Deserialize<'de> for ListWorkflowModelsResult {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        let document = ResultDocument::deserialize(deserializer)?;
        Self::new(document.matches, document.next_continuation_token).map_err(Source::Error::custom)
    }
}
