//! What is running, what is stuck, and what already ran.
//!
//! Active instances and archived ones are the same question asked about
//! different states, so this is one command with a state set rather than two
//! commands whose answers a caller would have to reconcile. `completed` and
//! `aborted` are the archived instances.
//!
//! The state set is required rather than optional. "Every instance a deployment
//! has ever run" is not a question anybody means to ask, and making it the
//! default would make it the question everybody asks by accident.

use serde::de::Error as _;
use serde::{Deserialize, Serialize};

use crate::command::operational_listing::{ListingResultFailure, require_strictly_ascending_text};
use crate::command::process_identity::{
    RequestedWorkflowInstanceStates, WorkflowInstanceIdentifier, WorkflowInstanceState,
    WorkflowModelIdentifier,
};
use crate::command::property_value::DateTimeString;
use crate::command::query_paths::anchor_contains;
use crate::command::repository_path::RepositoryPath;
use crate::command::result_window::{ContinuationToken, ResultWindow};

/// One request to find workflow instances.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FindWorkflowInstancesCommand {
    /// Model a reported instance must run, when the caller said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_identifier: Option<WorkflowModelIdentifier>,
    /// Anchor a reported instance's payload must lie under, when the caller
    /// said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload_prefix: Option<RepositoryPath>,
    /// Page the caller is asking for, when the caller said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_window: Option<ResultWindow>,
    /// States a reported instance may be in.
    pub states: RequestedWorkflowInstanceStates,
}

impl FindWorkflowInstancesCommand {
    /// Returns the page this request asks for, stated or resolved.
    #[must_use]
    pub fn resolved_window(&self) -> ResultWindow {
        self.result_window.clone().unwrap_or_default()
    }

    /// Returns whether an instance so described is one this request asked about.
    #[must_use]
    pub fn admits(
        &self,
        model_identifier: &WorkflowModelIdentifier,
        payload_path: &RepositoryPath,
        state: WorkflowInstanceState,
    ) -> bool {
        let modelled = self.model_identifier.as_ref().is_none_or(|asked| asked == model_identifier);
        let under =
            self.payload_prefix.as_ref().is_none_or(|prefix| anchor_contains(prefix, payload_path));
        modelled && under && self.states.contains(state)
    }
}

/// One workflow instance the author reports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowInstanceMatch {
    /// The instance itself.
    pub instance_identifier: WorkflowInstanceIdentifier,
    /// The model it runs.
    pub model_identifier: WorkflowModelIdentifier,
    /// The content it runs on.
    pub payload_path: RepositoryPath,
    /// When it started, when the author reports it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTimeString>,
    /// What state it is in.
    pub state: WorkflowInstanceState,
}

/// One page of workflow instances.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FindWorkflowInstancesResult {
    /// Matches, strictly ascending by instance identifier bytes.
    pub matches: Vec<WorkflowInstanceMatch>,
    /// Where the next page resumes, when there is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_continuation_token: Option<ContinuationToken>,
}

impl FindWorkflowInstancesResult {
    /// Returns the page these matches describe.
    ///
    /// # Errors
    ///
    /// Returns [`ListingResultFailure::NotStrictlyAscending`] when an identifier
    /// repeats or sorts before its predecessor.
    pub fn new(
        matches: Vec<WorkflowInstanceMatch>,
        next_continuation_token: Option<ContinuationToken>,
    ) -> Result<Self, ListingResultFailure> {
        require_strictly_ascending_text(
            matches.iter().map(|found| found.instance_identifier.as_text()),
        )?;
        Ok(Self { matches, next_continuation_token })
    }

    /// Requires this page to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`ListingResultFailure::NotThisRequest`] when a match is outside
    /// the model, the payload anchor, or the states the command asked about.
    pub fn require_answers(
        &self,
        command: &FindWorkflowInstancesCommand,
    ) -> Result<(), ListingResultFailure> {
        let admitted = self
            .matches
            .iter()
            .all(|found| command.admits(&found.model_identifier, &found.payload_path, found.state));
        if admitted { Ok(()) } else { Err(ListingResultFailure::NotThisRequest) }
    }
}

/// One page exactly as it is written on the wire.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ResultDocument {
    /// Matches this page carries.
    matches: Vec<WorkflowInstanceMatch>,
    /// Where the next page resumes.
    #[serde(default)]
    next_continuation_token: Option<ContinuationToken>,
}

impl<'de> Deserialize<'de> for FindWorkflowInstancesResult {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        let document = ResultDocument::deserialize(deserializer)?;
        Self::new(document.matches, document.next_continuation_token).map_err(Source::Error::custom)
    }
}
