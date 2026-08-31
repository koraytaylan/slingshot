//! What replication agents exist, and what state each one is in.
//!
//! `replicate_content` offers content to replication and nothing could then ask
//! what replication did with it. These two reads answer that at two depths and
//! land together because they share the rule that shapes both.
//!
//! # An agent's transport address is a credential
//!
//! A publish agent's transport carries the user name and password it
//! authenticates to a publisher with, and a flush agent's carries the dispatcher
//! it can invalidate. Neither result reports the address. What they report is a
//! closed transport kind derived from what sort of agent it is, which answers
//! "is this the publish agent or the flush agent" without answering "what are
//! its credentials". The types have no member that could hold an address, so
//! this is structural rather than a promise somebody keeps remembering.

use serde::de::Error as _;
use serde::{Deserialize, Serialize};

use crate::command::command_identity::CommandContract;
use crate::command::find_pages_containing_phrase::PageTitle;
use crate::command::operational_listing::{ListingResultFailure, require_strictly_ascending_text};
use crate::command::platform_service_identity::{
    ReplicationAgentIdentifier, ReplicationTransportKind,
};
use crate::command::repository_path::RepositoryPath;
use crate::command::result_window::{ContinuationToken, ResultWindow};

/// One request to list replication agents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListReplicationAgentsCommand {
    /// Page the caller is asking for, when the caller said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_window: Option<ResultWindow>,
}

impl ListReplicationAgentsCommand {
    /// Returns the page this request asks for, stated or resolved.
    #[must_use]
    pub fn resolved_window(&self) -> ResultWindow {
        self.result_window.clone().unwrap_or_default()
    }
}

/// One request to inspect one replication agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InspectReplicationAgentCommand {
    /// Agent to inspect.
    pub agent_identifier: ReplicationAgentIdentifier,
}

/// One replication agent, as a listing describes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReplicationAgentMatch {
    /// The agent itself.
    pub agent_identifier: ReplicationAgentIdentifier,
    /// Whether it is switched on.
    pub enabled: bool,
    /// Whether its queue has stopped moving.
    pub queue_blocked: bool,
    /// How many entries are waiting in it.
    pub queued_entry_count: u64,
    /// Where the agent is configured.
    pub repository_path: RepositoryPath,
    /// What the agent is for.
    pub title: PageTitle,
    /// What sort of transport it is built on, and never where it points.
    pub transport_kind: ReplicationTransportKind,
}

impl ReplicationAgentMatch {
    /// Returns the row these facts describe.
    ///
    /// # Errors
    ///
    /// Returns [`ListingResultFailure::TooManyRequested`] when the queued count
    /// exceeds what the contract permits one queue to hold.
    pub fn new(
        agent_identifier: ReplicationAgentIdentifier,
        enabled: bool,
        queue_blocked: bool,
        queued_entry_count: u64,
        repository_path: RepositoryPath,
        title: PageTitle,
        transport_kind: ReplicationTransportKind,
    ) -> Result<Self, ListingResultFailure> {
        require_queue_within(queued_entry_count)?;
        Ok(Self {
            agent_identifier,
            enabled,
            queue_blocked,
            queued_entry_count,
            repository_path,
            title,
            transport_kind,
        })
    }
}

/// One replication agent, in full.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InspectReplicationAgentResult {
    /// The agent itself.
    pub agent_identifier: ReplicationAgentIdentifier,
    /// Whether it is switched on.
    pub enabled: bool,
    /// Whether its queue has stopped moving.
    pub queue_blocked: bool,
    /// How many entries are waiting in it.
    pub queued_entry_count: u64,
    /// Where the agent is configured.
    pub repository_path: RepositoryPath,
    /// How long it waits before trying a failed entry again.
    pub retry_delay_milliseconds: u64,
    /// What the agent is for.
    pub title: PageTitle,
    /// What sort of transport it is built on, and never where it points.
    pub transport_kind: ReplicationTransportKind,
}

impl InspectReplicationAgentResult {
    /// Requires this result to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`ListingResultFailure::NotThisRequest`] when it names another
    /// request's agent, and [`ListingResultFailure::TooManyRequested`] when the
    /// queued count exceeds what one queue may hold.
    pub fn require_answers(
        &self,
        command: &InspectReplicationAgentCommand,
    ) -> Result<(), ListingResultFailure> {
        require_queue_within(self.queued_entry_count)?;
        if self.agent_identifier == command.agent_identifier {
            Ok(())
        } else {
            Err(ListingResultFailure::NotThisRequest)
        }
    }
}

/// Why an agent could not be listed or inspected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplicationAgentFailure {
    /// No agent answers to that identifier.
    AgentNotFound,
    /// The agent is there and this caller may not read it.
    AgentAccessDenied,
    /// The replication service could not be reached.
    AgentInventoryFailed,
}

/// One refused agent read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplicationAgentRefusal {
    /// Agent this request named.
    pub agent_identifier: ReplicationAgentIdentifier,
    /// Why it was refused.
    pub failure: ReplicationAgentFailure,
}

/// One page of replication agents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ListReplicationAgentsResult {
    /// Matches, strictly ascending by agent identifier bytes.
    pub matches: Vec<ReplicationAgentMatch>,
    /// Where the next page resumes, when there is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_continuation_token: Option<ContinuationToken>,
}

impl ListReplicationAgentsResult {
    /// Returns the page these matches describe.
    ///
    /// # Errors
    ///
    /// Returns [`ListingResultFailure::NotStrictlyAscending`] when an identifier
    /// repeats or sorts before its predecessor.
    pub fn new(
        matches: Vec<ReplicationAgentMatch>,
        next_continuation_token: Option<ContinuationToken>,
    ) -> Result<Self, ListingResultFailure> {
        require_strictly_ascending_text(
            matches.iter().map(|found| found.agent_identifier.as_text()),
        )?;
        Ok(Self { matches, next_continuation_token })
    }
}

/// Requires one queued count to be within the contract's bound.
fn require_queue_within(count: u64) -> Result<(), ListingResultFailure> {
    if count > CommandContract::embedded().limit("maximum_replication_queue_entries") {
        return Err(ListingResultFailure::TooManyRequested);
    }
    Ok(())
}

/// One agent exactly as it is written on the wire.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AgentDocument {
    /// The agent itself.
    agent_identifier: ReplicationAgentIdentifier,
    /// Whether it is switched on.
    enabled: bool,
    /// Whether its queue has stopped moving.
    queue_blocked: bool,
    /// How many entries are waiting in it.
    queued_entry_count: u64,
    /// Where the agent is configured.
    repository_path: RepositoryPath,
    /// What the agent is for.
    title: PageTitle,
    /// What sort of transport it is built on.
    transport_kind: ReplicationTransportKind,
}

impl<'de> Deserialize<'de> for ReplicationAgentMatch {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        let document = AgentDocument::deserialize(deserializer)?;
        Self::new(
            document.agent_identifier,
            document.enabled,
            document.queue_blocked,
            document.queued_entry_count,
            document.repository_path,
            document.title,
            document.transport_kind,
        )
        .map_err(Source::Error::custom)
    }
}

/// One inspection exactly as it is written on the wire.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct InspectionDocument {
    /// The agent itself.
    agent_identifier: ReplicationAgentIdentifier,
    /// Whether it is switched on.
    enabled: bool,
    /// Whether its queue has stopped moving.
    queue_blocked: bool,
    /// How many entries are waiting in it.
    queued_entry_count: u64,
    /// Where the agent is configured.
    repository_path: RepositoryPath,
    /// How long it waits before trying a failed entry again.
    retry_delay_milliseconds: u64,
    /// What the agent is for.
    title: PageTitle,
    /// What sort of transport it is built on.
    transport_kind: ReplicationTransportKind,
}

impl<'de> Deserialize<'de> for InspectReplicationAgentResult {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        let document = InspectionDocument::deserialize(deserializer)?;
        require_queue_within(document.queued_entry_count).map_err(Source::Error::custom)?;
        Ok(Self {
            agent_identifier: document.agent_identifier,
            enabled: document.enabled,
            queue_blocked: document.queue_blocked,
            queued_entry_count: document.queued_entry_count,
            repository_path: document.repository_path,
            retry_delay_milliseconds: document.retry_delay_milliseconds,
            title: document.title,
            transport_kind: document.transport_kind,
        })
    }
}

/// One page exactly as it is written on the wire.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ResultDocument {
    /// Matches this page carries.
    matches: Vec<ReplicationAgentMatch>,
    /// Where the next page resumes.
    #[serde(default)]
    next_continuation_token: Option<ContinuationToken>,
}

impl<'de> Deserialize<'de> for ListReplicationAgentsResult {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        let document = ResultDocument::deserialize(deserializer)?;
        Self::new(document.matches, document.next_continuation_token).map_err(Source::Error::custom)
    }
}
