//! Emptying a replication queue, and the guard that makes it safe to.
//!
//! This throws away work an author accepted, which makes it the most destructive
//! command in the family, and it is used under the most pressure - a queue is
//! blocked, publishing has stopped, and somebody wants it moving again.
//!
//! That is exactly when a queue grows between looking at it and acting on it. So
//! the request may state the number of entries it expects, the expectation is
//! checked before anything is removed, and a mismatch is a refusal that proves
//! no effect. The caller looks again and decides again, rather than discovering
//! afterwards that it emptied more than it saw.

use serde::{Deserialize, Serialize};

use crate::command::command_identity::CommandContract;
use crate::command::platform_service_identity::ReplicationAgentIdentifier;
use crate::command::resource_mutation::MutationResultFailure;

/// One request to empty an agent's queue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlushReplicationQueueCommand {
    /// Agent whose queue is emptied.
    pub agent_identifier: ReplicationAgentIdentifier,
    /// How many entries the caller believes are in it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_entry_count: Option<u64>,
}

impl FlushReplicationQueueCommand {
    /// Requires the stated expectation to be within the contract's bound.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::CountTooLarge`] when the expectation
    /// exceeds what one queue may hold.
    pub fn require_usable(&self) -> Result<(), MutationResultFailure> {
        let bound = CommandContract::embedded().limit("maximum_replication_queue_entries");
        if self.expected_entry_count.is_some_and(|expected| expected > bound) {
            return Err(MutationResultFailure::CountTooLarge);
        }
        Ok(())
    }
}

/// Why a queue was not emptied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlushReplicationQueueFailure {
    /// No agent answers to that identifier.
    AgentNotFound,
    /// The agent is there and this caller may not empty its queue.
    AgentAccessDenied,
    /// The queue holds a different number of entries than the request expected.
    QueueExpectationMismatch,
    /// The author refused to empty it.
    PlatformControlRejected,
    /// Nobody can tell whether it was emptied.
    PlatformControlOutcomeUnknown,
}

/// One refused flush.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlushReplicationQueueRefusal {
    /// Agent this request named.
    pub agent_identifier: ReplicationAgentIdentifier,
    /// Why it was refused.
    pub failure: FlushReplicationQueueFailure,
}

impl FlushReplicationQueueRefusal {
    /// Returns whether this refusal proves the operation changed nothing.
    #[must_use]
    pub fn proves_no_effect(&self) -> bool {
        !matches!(self.failure, FlushReplicationQueueFailure::PlatformControlOutcomeUnknown)
    }

    /// Requires this refusal to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::NotThisRequest`] when it names another
    /// request's agent, and when it reports an expectation mismatch for a
    /// request that stated no expectation.
    pub fn require_answers(
        &self,
        command: &FlushReplicationQueueCommand,
    ) -> Result<(), MutationResultFailure> {
        let mismatched =
            matches!(self.failure, FlushReplicationQueueFailure::QueueExpectationMismatch);
        if self.agent_identifier != command.agent_identifier
            || (mismatched && command.expected_entry_count.is_none())
        {
            return Err(MutationResultFailure::NotThisRequest);
        }
        Ok(())
    }
}

/// What emptying a queue removed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlushReplicationQueueResult {
    /// Agent whose queue was emptied.
    pub agent_identifier: ReplicationAgentIdentifier,
    /// How many entries went.
    pub removed_entry_count: u64,
}

impl FlushReplicationQueueResult {
    /// Requires this result to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::NotThisRequest`] when it names another
    /// request's agent, or removes a different number than the request
    /// expected: fewer is the silent under-flush the expectation exists to
    /// catch, and more is the over-flush. Returns
    /// [`MutationResultFailure::CountTooLarge`] above the contract's queue
    /// bound.
    pub fn require_answers(
        &self,
        command: &FlushReplicationQueueCommand,
    ) -> Result<(), MutationResultFailure> {
        let bound = CommandContract::embedded().limit("maximum_replication_queue_entries");
        if self.removed_entry_count > bound {
            return Err(MutationResultFailure::CountTooLarge);
        }
        let disagreed = command
            .expected_entry_count
            .is_some_and(|expected| self.removed_entry_count != expected);
        if self.agent_identifier != command.agent_identifier || disagreed {
            return Err(MutationResultFailure::NotThisRequest);
        }
        Ok(())
    }
}
