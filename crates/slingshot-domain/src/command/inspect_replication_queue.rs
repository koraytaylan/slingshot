//! What is stuck at the head of a replication queue.
//!
//! A blocked queue is why published content does not appear, and the entry at
//! its head is why the queue is blocked. This lists one agent's queue as a page
//! of entries.
//!
//! The blocked state is on the page rather than on every row, because it is a
//! fact about the queue. Repeating it per entry would invite two answers to one
//! question, and the row where they disagreed would be the interesting one.

use serde::de::Error as _;
use serde::{Deserialize, Serialize};

use crate::command::command_identity::CommandContract;
use crate::command::operational_listing::{ListingResultFailure, require_strictly_ascending_text};
use crate::command::platform_service_identity::{
    ReplicationAction, ReplicationAgentIdentifier, ReplicationQueueEntryIdentifier,
};
use crate::command::repository_path::RepositoryPath;
use crate::command::result_window::{ContinuationToken, ResultWindow};

/// One request to inspect an agent's queue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InspectReplicationQueueCommand {
    /// Agent whose queue is listed.
    pub agent_identifier: ReplicationAgentIdentifier,
    /// Page the caller is asking for, when the caller said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_window: Option<ResultWindow>,
}

impl InspectReplicationQueueCommand {
    /// Returns the page this request asks for, stated or resolved.
    #[must_use]
    pub fn resolved_window(&self) -> ResultWindow {
        self.result_window.clone().unwrap_or_default()
    }
}

/// One entry waiting in a replication queue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplicationQueueEntry {
    /// What the entry asks for.
    pub action: ReplicationAction,
    /// How many times it has been tried.
    pub attempt_count: u64,
    /// The content it carries.
    pub content_path: RepositoryPath,
    /// The entry itself.
    pub entry_identifier: ReplicationQueueEntryIdentifier,
    /// What went wrong last time, when something did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_failure_category: Option<String>,
}

/// One page of a replication queue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InspectReplicationQueueResult {
    /// Whether the queue has stopped moving.
    pub blocked: bool,
    /// Entries, strictly ascending by entry identifier bytes.
    pub entries: Vec<ReplicationQueueEntry>,
    /// Where the next page resumes, when there is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_continuation_token: Option<ContinuationToken>,
}

impl InspectReplicationQueueResult {
    /// Returns the page these entries describe.
    ///
    /// # Errors
    ///
    /// Returns [`ListingResultFailure::TooManyRequested`] above the contract's
    /// queue bound and [`ListingResultFailure::NotStrictlyAscending`] when an
    /// entry repeats or sorts before its predecessor.
    pub fn new(
        blocked: bool,
        entries: Vec<ReplicationQueueEntry>,
        next_continuation_token: Option<ContinuationToken>,
    ) -> Result<Self, ListingResultFailure> {
        let bound = CommandContract::embedded().limit("maximum_replication_queue_entries");
        if u64::try_from(entries.len()).unwrap_or(u64::MAX) > bound {
            return Err(ListingResultFailure::TooManyRequested);
        }
        require_strictly_ascending_text(
            entries.iter().map(|entry| entry.entry_identifier.as_text()),
        )?;
        Ok(Self { blocked, entries, next_continuation_token })
    }
}

/// One page exactly as it is written on the wire.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ResultDocument {
    /// Whether the queue has stopped moving.
    blocked: bool,
    /// Entries this page carries.
    entries: Vec<ReplicationQueueEntry>,
    /// Where the next page resumes.
    #[serde(default)]
    next_continuation_token: Option<ContinuationToken>,
}

impl<'de> Deserialize<'de> for InspectReplicationQueueResult {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        let document = ResultDocument::deserialize(deserializer)?;
        Self::new(document.blocked, document.entries, document.next_continuation_token)
            .map_err(Source::Error::custom)
    }
}
