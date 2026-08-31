//! Why one workflow instance is not moving.
//!
//! A listing can say an instance is running and cannot say what it is waiting
//! for. A stalled workflow is stalled at a work item, so this command reports
//! the open work items and who each is assigned to.
//!
//! An assignee is an authorizable identifier and never a display name. The
//! identifier is what every other command in this registry addresses a person
//! by, and a display name is a person's name in a diagnostic that has no reason
//! to carry one.

use serde::de::Error as _;
use serde::{Deserialize, Serialize};

use crate::command::authorizable_identity::AuthorizableIdentifier;
use crate::command::command_identity::CommandContract;
use crate::command::find_pages_containing_phrase::PageTitle;
use crate::command::operational_listing::{ListingResultFailure, require_strictly_ascending_text};
use crate::command::process_identity::{
    WorkItemIdentifier, WorkflowInstanceIdentifier, WorkflowInstanceState, WorkflowModelIdentifier,
};
use crate::command::repository_path::RepositoryPath;

/// One request to inspect a workflow instance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InspectWorkflowInstanceCommand {
    /// Instance to inspect.
    pub instance_identifier: WorkflowInstanceIdentifier,
}

/// One open work item of a workflow instance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkItem {
    /// Who it is assigned to, when it is assigned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assignee: Option<AuthorizableIdentifier>,
    /// The title of the step it is waiting at.
    pub node_title: PageTitle,
    /// The work item itself.
    pub work_item_identifier: WorkItemIdentifier,
}

/// Why an instance could not be inspected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InspectWorkflowInstanceFailure {
    /// No instance answers to that identifier.
    InstanceNotFound,
    /// The instance is there and this caller may not read it.
    InstanceAccessDenied,
    /// The workflow service could not be reached.
    WorkflowInventoryFailed,
    /// The instance holds more than this contract will return at once.
    ResultBudgetExceeded,
}

/// One refused inspection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InspectWorkflowInstanceRefusal {
    /// Why it was refused.
    pub failure: InspectWorkflowInstanceFailure,
    /// Instance this request named.
    pub instance_identifier: WorkflowInstanceIdentifier,
}

impl InspectWorkflowInstanceRefusal {
    /// Requires this refusal to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`ListingResultFailure::NotThisRequest`] when it names another
    /// request's instance.
    pub fn require_answers(
        &self,
        command: &InspectWorkflowInstanceCommand,
    ) -> Result<(), ListingResultFailure> {
        if self.instance_identifier == command.instance_identifier {
            Ok(())
        } else {
            Err(ListingResultFailure::NotThisRequest)
        }
    }
}

/// What one workflow instance is doing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InspectWorkflowInstanceResult {
    /// The instance itself.
    pub instance_identifier: WorkflowInstanceIdentifier,
    /// The model it runs.
    pub model_identifier: WorkflowModelIdentifier,
    /// The content it runs on.
    pub payload_path: RepositoryPath,
    /// What state it is in.
    pub state: WorkflowInstanceState,
    /// Its open work items, ascending by identifier.
    pub work_items: Vec<WorkItem>,
}

impl InspectWorkflowInstanceResult {
    /// Returns the inspection these facts describe.
    ///
    /// # Errors
    ///
    /// Returns [`ListingResultFailure::TooManyRequested`] above the contract's
    /// work-item bound and
    /// [`ListingResultFailure::NotStrictlyAscending`] when a work item repeats
    /// or sorts before its predecessor.
    pub fn new(
        instance_identifier: WorkflowInstanceIdentifier,
        model_identifier: WorkflowModelIdentifier,
        payload_path: RepositoryPath,
        state: WorkflowInstanceState,
        work_items: Vec<WorkItem>,
    ) -> Result<Self, ListingResultFailure> {
        let bound = CommandContract::embedded().limit("maximum_workflow_work_items");
        if u64::try_from(work_items.len()).unwrap_or(u64::MAX) > bound {
            return Err(ListingResultFailure::TooManyRequested);
        }
        require_strictly_ascending_text(
            work_items.iter().map(|item| item.work_item_identifier.as_text()),
        )?;
        Ok(Self { instance_identifier, model_identifier, payload_path, state, work_items })
    }

    /// Requires this result to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`ListingResultFailure::NotThisRequest`] when it names another
    /// request's instance.
    pub fn require_answers(
        &self,
        command: &InspectWorkflowInstanceCommand,
    ) -> Result<(), ListingResultFailure> {
        if self.instance_identifier == command.instance_identifier {
            Ok(())
        } else {
            Err(ListingResultFailure::NotThisRequest)
        }
    }
}

/// One inspection exactly as it is written on the wire.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ResultDocument {
    /// The instance itself.
    instance_identifier: WorkflowInstanceIdentifier,
    /// The model it runs.
    model_identifier: WorkflowModelIdentifier,
    /// The content it runs on.
    payload_path: RepositoryPath,
    /// What state it is in.
    state: WorkflowInstanceState,
    /// Its open work items.
    work_items: Vec<WorkItem>,
}

impl<'de> Deserialize<'de> for InspectWorkflowInstanceResult {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        let document = ResultDocument::deserialize(deserializer)?;
        Self::new(
            document.instance_identifier,
            document.model_identifier,
            document.payload_path,
            document.state,
            document.work_items,
        )
        .map_err(Source::Error::custom)
    }
}
