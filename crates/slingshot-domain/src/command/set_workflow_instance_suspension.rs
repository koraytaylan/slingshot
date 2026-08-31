//! Holding a workflow instance, and letting it go again.
//!
//! Suspending and resuming are one decision with two values, so they are one
//! command. Two commands would be two places for the same state machine to
//! disagree with itself, and the disagreement would only show up on the instance
//! somebody was already worried about.
//!
//! The requested state is the closed pair `suspended` and `running`, and the
//! answer is the state the author observed. A terminated instance observed after
//! a request to resume is a real observation, and this contract reports it
//! rather than refusing the result for not being one of the two.

use serde::{Deserialize, Serialize};

use crate::command::process_identity::{WorkflowInstanceIdentifier, WorkflowInstanceState};
use crate::command::resource_mutation::MutationResultFailure;

/// What one request asks an instance to become.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestedSuspension {
    /// Let it advance again.
    Running,
    /// Hold it where it is.
    Suspended,
}

/// One request to hold or release a workflow instance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetWorkflowInstanceSuspensionCommand {
    /// Instance to act on.
    pub instance_identifier: WorkflowInstanceIdentifier,
    /// What to ask it to become.
    pub requested_state: RequestedSuspension,
}

/// Why an instance was not held or released.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SetWorkflowInstanceSuspensionFailure {
    /// No instance answers to that identifier.
    InstanceNotFound,
    /// The instance is there and this caller may not act on it.
    InstanceAccessDenied,
    /// The instance has ended, so it can be neither held nor released.
    InstanceNotSuspendable,
    /// The author refused the change.
    PlatformControlRejected,
    /// Nobody can tell whether the change took effect.
    PlatformControlOutcomeUnknown,
}

/// One refused suspension change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetWorkflowInstanceSuspensionRefusal {
    /// Why it was refused.
    pub failure: SetWorkflowInstanceSuspensionFailure,
    /// Instance this request named.
    pub instance_identifier: WorkflowInstanceIdentifier,
}

impl SetWorkflowInstanceSuspensionRefusal {
    /// Returns whether this refusal proves the operation changed nothing.
    #[must_use]
    pub fn proves_no_effect(&self) -> bool {
        !matches!(self.failure, SetWorkflowInstanceSuspensionFailure::PlatformControlOutcomeUnknown)
    }

    /// Requires this refusal to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::NotThisRequest`] when it names another
    /// request's instance.
    pub fn require_answers(
        &self,
        command: &SetWorkflowInstanceSuspensionCommand,
    ) -> Result<(), MutationResultFailure> {
        if self.instance_identifier == command.instance_identifier {
            Ok(())
        } else {
            Err(MutationResultFailure::NotThisRequest)
        }
    }
}

/// What the author observed after the change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetWorkflowInstanceSuspensionResult {
    /// Instance that was acted on.
    pub instance_identifier: WorkflowInstanceIdentifier,
    /// The state it was in afterwards.
    pub observed_state: WorkflowInstanceState,
}

impl SetWorkflowInstanceSuspensionResult {
    /// Requires this result to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::NotThisRequest`] when it names another
    /// request's instance.
    pub fn require_answers(
        &self,
        command: &SetWorkflowInstanceSuspensionCommand,
    ) -> Result<(), MutationResultFailure> {
        if self.instance_identifier == command.instance_identifier {
            Ok(())
        } else {
            Err(MutationResultFailure::NotThisRequest)
        }
    }
}
