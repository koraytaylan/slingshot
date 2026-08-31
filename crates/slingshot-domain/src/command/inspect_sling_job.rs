//! One job in full, minus the one thing it must not carry.
//!
//! A job's properties are what an operator wants to see and are also where a
//! deployment puts whatever it likes: a path, a user name, an API key. This
//! command reports the keys in ascending order and no values at all, for the
//! reason the configuration listing reports none - it has made no judgement
//! about which of them would be safe to read, so it reads none.
//!
//! The retry counts are both reported because one without the other says
//! nothing: three retries out of three is exhausted, and three out of ten is
//! still trying.

use serde::de::Error as _;
use serde::{Deserialize, Serialize};

use crate::command::command_identity::CommandContract;
use crate::command::operational_listing::{ListingResultFailure, require_strictly_ascending_text};
use crate::command::process_identity::{
    SlingJobIdentifier, SlingJobQueueName, SlingJobState, SlingJobTopic,
};
use crate::command::repository_path::PropertyName;

/// One request to inspect a Sling job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InspectSlingJobCommand {
    /// Job to inspect.
    pub job_identifier: SlingJobIdentifier,
}

/// Why a job could not be inspected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InspectSlingJobFailure {
    /// No job answers to that identifier.
    JobNotFound,
    /// The job manager could not be reached.
    JobInventoryFailed,
    /// The job holds more than this contract will return at once.
    ResultBudgetExceeded,
}

/// One refused inspection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InspectSlingJobRefusal {
    /// Why it was refused.
    pub failure: InspectSlingJobFailure,
    /// Job this request named.
    pub job_identifier: SlingJobIdentifier,
}

impl InspectSlingJobRefusal {
    /// Requires this refusal to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`ListingResultFailure::NotThisRequest`] when it names another
    /// request's job.
    pub fn require_answers(
        &self,
        command: &InspectSlingJobCommand,
    ) -> Result<(), ListingResultFailure> {
        if self.job_identifier == command.job_identifier {
            Ok(())
        } else {
            Err(ListingResultFailure::NotThisRequest)
        }
    }
}

/// What one Sling job is, without what it holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InspectSlingJobResult {
    /// The job itself.
    pub job_identifier: SlingJobIdentifier,
    /// How many times it may be retried in total.
    pub maximum_retry_count: u64,
    /// The keys it carries, ascending. No values, by design.
    pub property_keys: Vec<PropertyName>,
    /// The queue that took it, when one has.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub queue_name: Option<SlingJobQueueName>,
    /// How many times it has been retried.
    pub retry_count: u64,
    /// What state it is in.
    pub state: SlingJobState,
    /// The topic it was dispatched on.
    pub topic: SlingJobTopic,
}

impl InspectSlingJobResult {
    /// Returns the inspection these facts describe.
    ///
    /// # Errors
    ///
    /// Returns [`ListingResultFailure::TooManyRequested`] above the contract's
    /// key bound and [`ListingResultFailure::NotStrictlyAscending`] when a key
    /// repeats or sorts before its predecessor.
    pub fn new(
        job_identifier: SlingJobIdentifier,
        maximum_retry_count: u64,
        property_keys: Vec<PropertyName>,
        queue_name: Option<SlingJobQueueName>,
        retry_count: u64,
        state: SlingJobState,
        topic: SlingJobTopic,
    ) -> Result<Self, ListingResultFailure> {
        let bound = CommandContract::embedded().limit("maximum_sling_job_property_keys");
        if u64::try_from(property_keys.len()).unwrap_or(u64::MAX) > bound {
            return Err(ListingResultFailure::TooManyRequested);
        }
        require_strictly_ascending_text(property_keys.iter().map(PropertyName::as_text))?;
        Ok(Self {
            job_identifier,
            maximum_retry_count,
            property_keys,
            queue_name,
            retry_count,
            state,
            topic,
        })
    }

    /// Requires this result to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`ListingResultFailure::NotThisRequest`] when it names another
    /// request's job.
    pub fn require_answers(
        &self,
        command: &InspectSlingJobCommand,
    ) -> Result<(), ListingResultFailure> {
        if self.job_identifier == command.job_identifier {
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
    /// The job itself.
    job_identifier: SlingJobIdentifier,
    /// How many times it may be retried in total.
    maximum_retry_count: u64,
    /// The keys it carries.
    property_keys: Vec<PropertyName>,
    /// The queue that took it.
    #[serde(default)]
    queue_name: Option<SlingJobQueueName>,
    /// How many times it has been retried.
    retry_count: u64,
    /// What state it is in.
    state: SlingJobState,
    /// The topic it was dispatched on.
    topic: SlingJobTopic,
}

impl<'de> Deserialize<'de> for InspectSlingJobResult {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        let document = ResultDocument::deserialize(deserializer)?;
        Self::new(
            document.job_identifier,
            document.maximum_retry_count,
            document.property_keys,
            document.queue_name,
            document.retry_count,
            document.state,
            document.topic,
        )
        .map_err(Source::Error::custom)
    }
}
