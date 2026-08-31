//! Workflow models, workflow instances, work items, and Sling jobs.
//!
//! A workflow model and a workflow instance are spelled as repository paths in
//! every deployment anybody has looked at, and neither is required to be. Giving
//! them the repository path grammar would be this contract asserting something
//! the author never promised, and would refuse a perfectly ordinary identifier
//! the day an author mints one differently. They are bounded opaque values.
//!
//! A Sling job topic is the opposite case: it does have a grammar the author
//! enforces, so it gets one here, and a caller learns about a malformed topic
//! before the request crosses a network rather than after.
//!
//! The closed states declare their variants in the byte order of their wire
//! spellings, so a requested set is ascending in the order a caller writes it.

use serde::{Deserialize, Serialize};

use crate::command::command_identity::CommandContract;
use crate::command::operational_listing::requested_states;
use crate::command::repository_path::{
    PathFailure, accept_opaque_body, accept_within, address_value,
};

/// Separator between two topic segments.
const TOPIC_SEPARATOR: char = '/';

/// States a workflow instance can be observed in.
pub const WORKFLOW_INSTANCE_STATE_COUNT: usize = 5;

/// States a Sling job can be observed in.
pub const SLING_JOB_STATE_COUNT: usize = 6;

/// States a Sling job queue can be observed in.
pub const SLING_JOB_QUEUE_STATE_COUNT: usize = 2;

address_value!(
    /// The identifier one workflow model is addressed by.
    WorkflowModelIdentifier,
    "workflow model identifier"
);

address_value!(
    /// The identifier one workflow instance is addressed by.
    WorkflowInstanceIdentifier,
    "workflow instance identifier"
);

address_value!(
    /// The identifier one work item is addressed by.
    WorkItemIdentifier,
    "work item identifier"
);

address_value!(
    /// The topic one Sling job is dispatched on.
    SlingJobTopic,
    "sling job topic"
);

address_value!(
    /// The identifier one Sling job is addressed by.
    SlingJobIdentifier,
    "sling job identifier"
);

address_value!(
    /// The name one Sling job queue is addressed by.
    SlingJobQueueName,
    "sling job queue name"
);

/// Declares one bounded opaque process identifier over its own limit.
macro_rules! opaque_identifier {
    ($name:ident, $limit:literal) => {
        impl $name {
            /// Validates one identifier.
            ///
            /// # Errors
            ///
            /// Returns [`PathFailure`] when the identifier is empty, longer than
            /// the contract allows, not already in normalization form C, carries
            /// a control, or has a leading or trailing ASCII space.
            pub fn parse(identifier: &str) -> Result<Self, PathFailure> {
                let bound = CommandContract::embedded().limit($limit);
                accept_within(identifier, bound, Self::role(), "bytes")?;
                accept_opaque_body(identifier, Self::role())?;
                Ok(Self::from_accepted(identifier))
            }
        }
    };
}

opaque_identifier!(WorkflowModelIdentifier, "maximum_workflow_model_identifier_bytes");
opaque_identifier!(WorkflowInstanceIdentifier, "maximum_workflow_instance_identifier_bytes");
opaque_identifier!(WorkItemIdentifier, "maximum_work_item_identifier_bytes");
opaque_identifier!(SlingJobIdentifier, "maximum_sling_job_identifier_bytes");
opaque_identifier!(SlingJobQueueName, "maximum_sling_job_queue_name_bytes");

impl SlingJobTopic {
    /// Validates one Sling job topic.
    ///
    /// # Errors
    ///
    /// Returns [`PathFailure`] when the topic is empty, longer than the contract
    /// allows, not already in normalization form C, begins or ends with a
    /// separator, or carries a segment that is empty or holds a character
    /// outside the topic alphabet.
    pub fn parse(topic: &str) -> Result<Self, PathFailure> {
        let bound = CommandContract::embedded().limit("maximum_sling_job_topic_bytes");
        accept_within(topic, bound, Self::role(), "bytes")?;
        let refuse = || PathFailure::at(Self::role(), "segment");
        if topic.starts_with(TOPIC_SEPARATOR) || topic.ends_with(TOPIC_SEPARATOR) {
            return Err(refuse());
        }
        for segment in topic.split(TOPIC_SEPARATOR) {
            if segment.is_empty() || !segment.chars().all(is_topic_character) {
                return Err(refuse());
            }
        }
        Ok(Self::from_accepted(topic))
    }

    /// Returns the segments this topic names, in order.
    #[must_use]
    pub fn segments(&self) -> Vec<&str> {
        self.as_text().split(TOPIC_SEPARATOR).collect()
    }
}

/// The state one workflow instance is in.
///
/// `Completed` and `Aborted` are the archived instances. They are members of
/// this set rather than a separate command's subject, because "show me what ran
/// last week" and "show me what is running" are the same question with a
/// different answer to one filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowInstanceState {
    /// Ended before its model finished.
    Aborted,
    /// Ended by reaching the end of its model.
    Completed,
    /// Advancing, or waiting on a work item.
    Running,
    /// Neither advancing nor ended, which an author reports and does not repair.
    Stale,
    /// Held, and resumable.
    Suspended,
}

impl WorkflowInstanceState {
    /// Returns every state, in the order they are written.
    #[must_use]
    pub fn every() -> [Self; WORKFLOW_INSTANCE_STATE_COUNT] {
        [Self::Aborted, Self::Completed, Self::Running, Self::Stale, Self::Suspended]
    }

    /// Reports whether an instance in this state has ended.
    #[must_use]
    pub fn has_ended(self) -> bool {
        matches!(self, Self::Aborted | Self::Completed)
    }
}

/// The state one Sling job is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SlingJobState {
    /// Running now.
    Active,
    /// Withdrawn before it ran.
    Cancelled,
    /// Discarded after exhausting its retries.
    Dropped,
    /// Ended in a failure that may be retried.
    Error,
    /// Waiting for a queue to take it.
    Queued,
    /// Ended without failing.
    Succeeded,
}

impl SlingJobState {
    /// Returns every state, in the order they are written.
    #[must_use]
    pub fn every() -> [Self; SLING_JOB_STATE_COUNT] {
        [Self::Active, Self::Cancelled, Self::Dropped, Self::Error, Self::Queued, Self::Succeeded]
    }

    /// Reports whether a job in this state can still be cancelled.
    #[must_use]
    pub fn is_cancellable(self) -> bool {
        matches!(self, Self::Active | Self::Queued)
    }
}

/// The state one Sling job queue is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SlingJobQueueState {
    /// Taking jobs.
    Running,
    /// Holding jobs without taking them.
    Suspended,
}

impl SlingJobQueueState {
    /// Returns every state, in the order they are written.
    #[must_use]
    pub fn every() -> [Self; SLING_JOB_QUEUE_STATE_COUNT] {
        [Self::Running, Self::Suspended]
    }
}

requested_states!(
    /// A nonempty ascending set of workflow instance states a search asks about.
    RequestedWorkflowInstanceStates,
    WorkflowInstanceState,
    "maximum_workflow_instance_states"
);

requested_states!(
    /// A nonempty ascending set of Sling job states a search asks about.
    RequestedSlingJobStates,
    SlingJobState,
    "maximum_sling_job_states"
);

/// Reports whether one character may appear in a topic segment.
fn is_topic_character(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '-' || character == '_' || character == '.'
}
