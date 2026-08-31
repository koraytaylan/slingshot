//! Starting workflows, finding them, and unsticking them.
//!
//! `--states` is required on the instance search and on nothing else, because
//! "every instance this deployment has ever run" is not a question anybody means
//! to ask and a default would make it the one everybody asks by accident.

use slingshot_domain::command::catalog::Command;
use slingshot_domain::command::find_pages_containing_phrase::PageTitle;
use slingshot_domain::command::find_workflow_instances::FindWorkflowInstancesCommand;
use slingshot_domain::command::inspect_workflow_instance::InspectWorkflowInstanceCommand;
use slingshot_domain::command::list_workflow_models::ListWorkflowModelsCommand;
use slingshot_domain::command::process_identity::{
    RequestedWorkflowInstanceStates, WorkflowInstanceIdentifier, WorkflowInstanceState,
    WorkflowModelIdentifier,
};
use slingshot_domain::command::set_workflow_instance_suspension::{
    RequestedSuspension, SetWorkflowInstanceSuspensionCommand,
};
use slingshot_domain::command::start_workflow::{StartWorkflowCommand, WorkflowMetadata};
use slingshot_domain::command::terminate_workflow_instance::TerminateWorkflowInstanceCommand;

use crate::commands::content::{RequestRefusal, require_key, required};
use crate::commands::operational_values::{
    list, optional_document, optional_path, optional_text, path, unusable,
};
use crate::commands::path_query::window;
use crate::invocation::{
    COMMENT_OPTION, INSTANCE_OPTION, Invocation, METADATA_OPTION, MODEL_OPTION, PATH_OPTION,
    PAYLOAD_PATH_OPTION, PREFIX_OPTION, STATES_OPTION, SUSPENSION_OPTION, TITLE_OPTION,
};

/// The wire name of the model listing.
pub const LIST_WORKFLOW_MODELS: &str = "list_workflow_models";

/// The wire name of the workflow start.
pub const START_WORKFLOW: &str = "start_workflow";

/// The wire name of the instance search.
pub const FIND_WORKFLOW_INSTANCES: &str = "find_workflow_instances";

/// The wire name of the instance inspection.
pub const INSPECT_WORKFLOW_INSTANCE: &str = "inspect_workflow_instance";

/// The wire name of the termination.
pub const TERMINATE_WORKFLOW_INSTANCE: &str = "terminate_workflow_instance";

/// The wire name of the suspension change.
pub const SET_WORKFLOW_INSTANCE_SUSPENSION: &str = "set_workflow_instance_suspension";

/// Every command this family builds.
const NAMES: &[&str] = &[
    LIST_WORKFLOW_MODELS,
    START_WORKFLOW,
    FIND_WORKFLOW_INSTANCES,
    INSPECT_WORKFLOW_INSTANCE,
    TERMINATE_WORKFLOW_INSTANCE,
    SET_WORKFLOW_INSTANCE_SUSPENSION,
];

/// Returns the typed request one invocation describes.
///
/// # Errors
///
/// Returns [`RequestRefusal`] naming the first thing that is wrong, or that this
/// family builds no such command.
pub fn build(invocation: &Invocation) -> Result<Command, RequestRefusal> {
    if !NAMES.contains(&invocation.verb.as_str()) {
        return Err(RequestRefusal::AnotherCommand { named: invocation.verb.clone() });
    }
    require_key(invocation)?;
    build_model(invocation).unwrap_or_else(|| build_instance(invocation))
}

/// Returns the model-facing command one invocation describes, when it is one.
fn build_model(invocation: &Invocation) -> Option<Result<Command, RequestRefusal>> {
    let built = match invocation.verb.as_str() {
        LIST_WORKFLOW_MODELS => list_models(invocation),
        START_WORKFLOW => start(invocation),
        FIND_WORKFLOW_INSTANCES => find_instances(invocation),
        _ => return None,
    };
    Some(built)
}

/// Returns the instance-facing command one invocation describes.
fn build_instance(invocation: &Invocation) -> Result<Command, RequestRefusal> {
    let instance_identifier = instance(invocation)?;
    match invocation.verb.as_str() {
        INSPECT_WORKFLOW_INSTANCE => {
            Ok(Command::InspectWorkflowInstance(InspectWorkflowInstanceCommand {
                instance_identifier,
            }))
        }
        TERMINATE_WORKFLOW_INSTANCE => {
            Ok(Command::TerminateWorkflowInstance(TerminateWorkflowInstanceCommand {
                instance_identifier,
            }))
        }
        _ => Ok(Command::SetWorkflowInstanceSuspension(SetWorkflowInstanceSuspensionCommand {
            instance_identifier,
            requested_state: suspension(invocation)?,
        })),
    }
}

/// Returns the model listing one invocation describes.
fn list_models(invocation: &Invocation) -> Result<Command, RequestRefusal> {
    let title_prefix = optional_text(invocation, PREFIX_OPTION)
        .map(|stated| PageTitle::new(stated).map_err(|_| unusable(PREFIX_OPTION)))
        .transpose()?;
    Ok(Command::ListWorkflowModels(ListWorkflowModelsCommand {
        result_window: window(invocation)?,
        title_prefix,
    }))
}

/// Returns the workflow start one invocation describes.
fn start(invocation: &Invocation) -> Result<Command, RequestRefusal> {
    let metadata: Option<WorkflowMetadata> = optional_document(invocation, METADATA_OPTION)?;
    let title = optional_text(invocation, TITLE_OPTION)
        .map(|stated| PageTitle::new(stated).map_err(|_| unusable(TITLE_OPTION)))
        .transpose()?;
    Ok(Command::StartWorkflow(StartWorkflowCommand {
        comment: optional_text(invocation, COMMENT_OPTION),
        metadata,
        model_identifier: model(invocation)?,
        payload_path: path(invocation, PAYLOAD_PATH_OPTION)?,
        title,
    }))
}

/// Returns the instance search one invocation describes.
fn find_instances(invocation: &Invocation) -> Result<Command, RequestRefusal> {
    let states: Vec<WorkflowInstanceState> = list(invocation, STATES_OPTION)?;
    let model_identifier = optional_text(invocation, MODEL_OPTION)
        .map(|stated| WorkflowModelIdentifier::parse(&stated).map_err(|_| unusable(MODEL_OPTION)))
        .transpose()?;
    Ok(Command::FindWorkflowInstances(FindWorkflowInstancesCommand {
        model_identifier,
        payload_prefix: optional_path(invocation, PATH_OPTION)?,
        result_window: window(invocation)?,
        states: RequestedWorkflowInstanceStates::new(states)
            .map_err(|_| unusable(STATES_OPTION))?,
    }))
}

/// Returns the model one invocation names.
fn model(invocation: &Invocation) -> Result<WorkflowModelIdentifier, RequestRefusal> {
    WorkflowModelIdentifier::parse(required(invocation, MODEL_OPTION)?)
        .map_err(|_| unusable(MODEL_OPTION))
}

/// Returns the instance one invocation names.
fn instance(invocation: &Invocation) -> Result<WorkflowInstanceIdentifier, RequestRefusal> {
    WorkflowInstanceIdentifier::parse(required(invocation, INSTANCE_OPTION)?)
        .map_err(|_| unusable(INSTANCE_OPTION))
}

/// Returns the suspension one invocation asks for.
fn suspension(invocation: &Invocation) -> Result<RequestedSuspension, RequestRefusal> {
    serde_json::from_str(&format!("\"{}\"", required(invocation, SUSPENSION_OPTION)?))
        .map_err(|_| unusable(SUSPENSION_OPTION))
}
