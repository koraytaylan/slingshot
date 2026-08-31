//! Ending a workflow instance that nobody is going to finish.
//!
//! An instance waiting on a step whose assignee has left, or on a step a removed
//! model no longer has, will wait forever. Ending it is destructive in this
//! registry's sense - something in effect stops being in effect - and the answer
//! is the state the author observed rather than the state the request aimed at,
//! for the reason a bundle transition answers that way: this contract reports
//! what the author saw and does not overrule it.

use serde::{Deserialize, Serialize};

use crate::command::process_identity::{WorkflowInstanceIdentifier, WorkflowInstanceState};
use crate::command::resource_mutation::MutationResultFailure;

/// One request to end a workflow instance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerminateWorkflowInstanceCommand {
    /// Instance to end.
    pub instance_identifier: WorkflowInstanceIdentifier,
}

/// Why an instance was not ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminateWorkflowInstanceFailure {
    /// No instance answers to that identifier.
    InstanceNotFound,
    /// The instance is there and this caller may not end it.
    InstanceAccessDenied,
    /// The instance has already ended.
    InstanceNotTerminable,
    /// The author refused to end it.
    PlatformControlRejected,
    /// Nobody can tell whether it ended.
    PlatformControlOutcomeUnknown,
}

/// One refused termination.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerminateWorkflowInstanceRefusal {
    /// Why it was refused.
    pub failure: TerminateWorkflowInstanceFailure,
    /// Instance this request named.
    pub instance_identifier: WorkflowInstanceIdentifier,
}

impl TerminateWorkflowInstanceRefusal {
    /// Returns whether this refusal proves the operation changed nothing.
    #[must_use]
    pub fn proves_no_effect(&self) -> bool {
        !matches!(self.failure, TerminateWorkflowInstanceFailure::PlatformControlOutcomeUnknown)
    }

    /// Requires this refusal to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::NotThisRequest`] when it names another
    /// request's instance.
    pub fn require_answers(
        &self,
        command: &TerminateWorkflowInstanceCommand,
    ) -> Result<(), MutationResultFailure> {
        if self.instance_identifier == command.instance_identifier {
            Ok(())
        } else {
            Err(MutationResultFailure::NotThisRequest)
        }
    }
}

/// What the author observed after the termination.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerminateWorkflowInstanceResult {
    /// Instance that was acted on.
    pub instance_identifier: WorkflowInstanceIdentifier,
    /// The state it was in afterwards.
    pub observed_state: WorkflowInstanceState,
}

impl TerminateWorkflowInstanceResult {
    /// Requires this result to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::NotThisRequest`] when it names another
    /// request's instance.
    pub fn require_answers(
        &self,
        command: &TerminateWorkflowInstanceCommand,
    ) -> Result<(), MutationResultFailure> {
        if self.instance_identifier == command.instance_identifier {
            Ok(())
        } else {
            Err(MutationResultFailure::NotThisRequest)
        }
    }
}
