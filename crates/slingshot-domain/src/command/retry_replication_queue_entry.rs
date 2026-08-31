//! Trying one queued entry again.
//!
//! The other half of a blocked queue. One entry failed for a reason that has
//! since been fixed - a publisher that was down, a path that was missing - and
//! until now the only way to move the queue was to empty it, which threw away
//! everything behind the entry too.
//!
//! The result says whether the entry was actually resubmitted, because an entry
//! that had already left the queue between looking and acting is a different
//! outcome from one that was retried, and a caller that could not tell them
//! apart would retry the wrong thing next.

use serde::{Deserialize, Serialize};

use crate::command::platform_service_identity::{
    ReplicationAgentIdentifier, ReplicationQueueEntryIdentifier,
};
use crate::command::resource_mutation::MutationResultFailure;

/// One request to retry a queued entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetryReplicationQueueEntryCommand {
    /// Agent whose queue holds it.
    pub agent_identifier: ReplicationAgentIdentifier,
    /// Entry to try again.
    pub entry_identifier: ReplicationQueueEntryIdentifier,
}

/// Why an entry was not retried.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetryReplicationQueueEntryFailure {
    /// No agent answers to that identifier.
    AgentNotFound,
    /// The agent is there and this caller may not act on its queue.
    AgentAccessDenied,
    /// No entry in that queue answers to that identifier.
    EntryNotFound,
    /// The author refused to retry it.
    PlatformControlRejected,
    /// Nobody can tell whether it was retried.
    PlatformControlOutcomeUnknown,
}

/// One refused retry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetryReplicationQueueEntryRefusal {
    /// Agent this request named.
    pub agent_identifier: ReplicationAgentIdentifier,
    /// Entry this request named.
    pub entry_identifier: ReplicationQueueEntryIdentifier,
    /// Why it was refused.
    pub failure: RetryReplicationQueueEntryFailure,
}

impl RetryReplicationQueueEntryRefusal {
    /// Returns whether this refusal proves the operation changed nothing.
    #[must_use]
    pub fn proves_no_effect(&self) -> bool {
        !matches!(self.failure, RetryReplicationQueueEntryFailure::PlatformControlOutcomeUnknown)
    }

    /// Requires this refusal to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::NotThisRequest`] when either identifier
    /// is another request's.
    pub fn require_answers(
        &self,
        command: &RetryReplicationQueueEntryCommand,
    ) -> Result<(), MutationResultFailure> {
        if self.agent_identifier == command.agent_identifier
            && self.entry_identifier == command.entry_identifier
        {
            Ok(())
        } else {
            Err(MutationResultFailure::NotThisRequest)
        }
    }
}

/// What retrying an entry did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetryReplicationQueueEntryResult {
    /// Agent whose queue holds it.
    pub agent_identifier: ReplicationAgentIdentifier,
    /// Entry that was acted on.
    pub entry_identifier: ReplicationQueueEntryIdentifier,
    /// Whether it was actually put back to be tried again.
    pub resubmitted: bool,
}

impl RetryReplicationQueueEntryResult {
    /// Requires this result to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::NotThisRequest`] when either identifier
    /// is another request's.
    pub fn require_answers(
        &self,
        command: &RetryReplicationQueueEntryCommand,
    ) -> Result<(), MutationResultFailure> {
        if self.agent_identifier == command.agent_identifier
            && self.entry_identifier == command.entry_identifier
        {
            Ok(())
        } else {
            Err(MutationResultFailure::NotThisRequest)
        }
    }
}
