//! The jobs behind a queue's numbers.
//!
//! A queue that says twelve jobs failed does not say which twelve, and the ones
//! that failed are the reason anybody is looking. This finds them by topic and
//! by state.
//!
//! The state set is required for the reason the workflow search requires one:
//! "every job this deployment has ever run" is not a question anybody means to
//! ask, and a default would make it the one everybody asks by accident.

use serde::de::Error as _;
use serde::{Deserialize, Serialize};

use crate::command::operational_listing::{ListingResultFailure, require_strictly_ascending_text};
use crate::command::process_identity::{
    RequestedSlingJobStates, SlingJobIdentifier, SlingJobQueueName, SlingJobState, SlingJobTopic,
};
use crate::command::result_window::{ContinuationToken, ResultWindow};

/// One request to find Sling jobs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FindSlingJobsCommand {
    /// Page the caller is asking for, when the caller said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_window: Option<ResultWindow>,
    /// States a reported job may be in.
    pub states: RequestedSlingJobStates,
    /// Topic a reported job must be dispatched on, when the caller said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topic: Option<SlingJobTopic>,
}

impl FindSlingJobsCommand {
    /// Returns the page this request asks for, stated or resolved.
    #[must_use]
    pub fn resolved_window(&self) -> ResultWindow {
        self.result_window.clone().unwrap_or_default()
    }

    /// Returns whether a job so dispatched and so stated is one this request
    /// asked about.
    #[must_use]
    pub fn admits(&self, topic: &SlingJobTopic, state: SlingJobState) -> bool {
        let dispatched = self.topic.as_ref().is_none_or(|asked| asked == topic);
        dispatched && self.states.contains(state)
    }
}

/// One Sling job the author reports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SlingJobMatch {
    /// The job itself.
    pub job_identifier: SlingJobIdentifier,
    /// The queue that took it, when one has.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queue_name: Option<SlingJobQueueName>,
    /// How many times it has been retried.
    pub retry_count: u64,
    /// What state it is in.
    pub state: SlingJobState,
    /// The topic it was dispatched on.
    pub topic: SlingJobTopic,
}

/// One page of Sling jobs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FindSlingJobsResult {
    /// Matches, strictly ascending by job identifier bytes.
    pub matches: Vec<SlingJobMatch>,
    /// Where the next page resumes, when there is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_continuation_token: Option<ContinuationToken>,
}

impl FindSlingJobsResult {
    /// Returns the page these matches describe.
    ///
    /// # Errors
    ///
    /// Returns [`ListingResultFailure::NotStrictlyAscending`] when an identifier
    /// repeats or sorts before its predecessor.
    pub fn new(
        matches: Vec<SlingJobMatch>,
        next_continuation_token: Option<ContinuationToken>,
    ) -> Result<Self, ListingResultFailure> {
        require_strictly_ascending_text(
            matches.iter().map(|found| found.job_identifier.as_text()),
        )?;
        Ok(Self { matches, next_continuation_token })
    }

    /// Requires this page to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`ListingResultFailure::NotThisRequest`] when a match is outside
    /// the topic or the states the command asked about.
    pub fn require_answers(
        &self,
        command: &FindSlingJobsCommand,
    ) -> Result<(), ListingResultFailure> {
        let admitted = self.matches.iter().all(|found| command.admits(&found.topic, found.state));
        if admitted { Ok(()) } else { Err(ListingResultFailure::NotThisRequest) }
    }
}

/// One page exactly as it is written on the wire.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ResultDocument {
    /// Matches this page carries.
    matches: Vec<SlingJobMatch>,
    /// Where the next page resumes.
    #[serde(default)]
    next_continuation_token: Option<ContinuationToken>,
}

impl<'de> Deserialize<'de> for FindSlingJobsResult {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        let document = ResultDocument::deserialize(deserializer)?;
        Self::new(document.matches, document.next_continuation_token).map_err(Source::Error::custom)
    }
}
