//! Which queues are taking work, and which are backed up.
//!
//! A suspended or backed-up queue explains every symptom downstream of it, and
//! it is the cheapest question in this family to answer, so it is the one the
//! other three are usually reached through.
//!
//! The two counts are separate on purpose. A queue with many active jobs is
//! busy; a queue with many queued jobs and none active is stuck. Reporting one
//! total would make those two indistinguishable, which is exactly the
//! distinction somebody is here to make.

use serde::de::Error as _;
use serde::{Deserialize, Serialize};

use crate::command::command_identity::CommandContract;
use crate::command::operational_listing::{ListingResultFailure, require_strictly_ascending_text};
use crate::command::process_identity::{SlingJobQueueName, SlingJobQueueState};
use crate::command::result_window::{ContinuationToken, ResultWindow};

/// One request to list Sling job queues.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListSlingJobQueuesCommand {
    /// Page the caller is asking for, when the caller said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_window: Option<ResultWindow>,
}

impl ListSlingJobQueuesCommand {
    /// Returns the page this request asks for, stated or resolved.
    #[must_use]
    pub fn resolved_window(&self) -> ResultWindow {
        self.result_window.clone().unwrap_or_default()
    }
}

/// One Sling job queue the author reports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SlingJobQueueMatch {
    /// How many jobs it is running now.
    pub active_job_count: u64,
    /// The name it is addressed by.
    pub queue_name: SlingJobQueueName,
    /// How many jobs are waiting for it.
    pub queued_job_count: u64,
    /// Whether it is taking jobs.
    pub state: SlingJobQueueState,
}

impl SlingJobQueueMatch {
    /// Returns the row these facts describe.
    ///
    /// # Errors
    ///
    /// Returns [`ListingResultFailure::TooManyRequested`] when either count
    /// exceeds what the contract permits one queue to report.
    pub fn new(
        active_job_count: u64,
        queue_name: SlingJobQueueName,
        queued_job_count: u64,
        state: SlingJobQueueState,
    ) -> Result<Self, ListingResultFailure> {
        let bound = CommandContract::embedded().limit("maximum_operational_candidate_records");
        if active_job_count > bound || queued_job_count > bound {
            return Err(ListingResultFailure::TooManyRequested);
        }
        Ok(Self { active_job_count, queue_name, queued_job_count, state })
    }
}

/// One page of Sling job queues.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ListSlingJobQueuesResult {
    /// Matches, strictly ascending by queue name bytes.
    pub matches: Vec<SlingJobQueueMatch>,
    /// Where the next page resumes, when there is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_continuation_token: Option<ContinuationToken>,
}

impl ListSlingJobQueuesResult {
    /// Returns the page these matches describe.
    ///
    /// # Errors
    ///
    /// Returns [`ListingResultFailure::NotStrictlyAscending`] when a queue name
    /// repeats or sorts before its predecessor.
    pub fn new(
        matches: Vec<SlingJobQueueMatch>,
        next_continuation_token: Option<ContinuationToken>,
    ) -> Result<Self, ListingResultFailure> {
        require_strictly_ascending_text(matches.iter().map(|found| found.queue_name.as_text()))?;
        Ok(Self { matches, next_continuation_token })
    }
}

/// One row exactly as it is written on the wire.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MatchDocument {
    /// How many jobs it is running now.
    active_job_count: u64,
    /// The name it is addressed by.
    queue_name: SlingJobQueueName,
    /// How many jobs are waiting for it.
    queued_job_count: u64,
    /// Whether it is taking jobs.
    state: SlingJobQueueState,
}

impl<'de> Deserialize<'de> for SlingJobQueueMatch {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        let document = MatchDocument::deserialize(deserializer)?;
        Self::new(
            document.active_job_count,
            document.queue_name,
            document.queued_job_count,
            document.state,
        )
        .map_err(Source::Error::custom)
    }
}

/// One page exactly as it is written on the wire.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ResultDocument {
    /// Matches this page carries.
    matches: Vec<SlingJobQueueMatch>,
    /// Where the next page resumes.
    #[serde(default)]
    next_continuation_token: Option<ContinuationToken>,
}

impl<'de> Deserialize<'de> for ListSlingJobQueuesResult {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        let document = ResultDocument::deserialize(deserializer)?;
        Self::new(document.matches, document.next_continuation_token).map_err(Source::Error::custom)
    }
}
