//! Complete event-stream document, distinct from its sequence/identity projection.
use crate::{identity::DocumentProvenance, job_contract::JobEventKind};
use serde::{Deserialize, Serialize};

/// Explicit logical state. Physical retries never imply a return to queued.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobEventState {
    /// Accepted, with no observed logical start.
    Queued,
    /// Logical execution has started.
    Running,
    /// Authoritative remote success.
    Succeeded,
    /// Remote failure, still requiring typed failure/result reconciliation.
    Failed,
}

/// Correlation carried only by a terminal event.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerminalCorrelation {
    /// Contracts under which this ending was produced.
    pub provenance: DocumentProvenance,
    /// Exact retained submission digest.
    pub submitted_command_digest: String,
}
impl core::fmt::Debug for TerminalCorrelation {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("TerminalCorrelation([redacted])")
    }
}

/// Complete closed wire envelope. Decoding the shape alone grants no authority;
/// the selected stream validates identity, bounds, state and terminal correlation.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobEventDocument {
    /// Incarnation of the event store.
    pub agent_event_store_generation: u64,
    /// Generation-scoped logical operation identity.
    pub agent_operation_identifier: String,
    /// Filtered subscription that delivered the event.
    pub daemon_subscription_identifier: String,
    /// Physical Sling job reporting this logical observation.
    pub sling_job_identifier: String,
    /// Event discriminator, checked against the explicit logical state.
    pub kind: JobEventKind,
    /// Explicit logical state, independent of physical retry mechanics.
    pub state: JobEventState,
    /// Per-operation event sequence, distinct from the SSE cursor.
    pub sequence: u64,
    /// Optional remote attempt. Omission preserves the retained value, not zero.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_counter"
    )]
    pub attempt: Option<u64>,
    /// Optional logical progress. Omission preserves the retained value, not zero.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_counter"
    )]
    pub progress: Option<u64>,
    /// Required for terminal kinds and forbidden for other kinds after validation.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_terminal"
    )]
    pub terminal: Option<TerminalCorrelation>,
}
impl core::fmt::Debug for JobEventDocument {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("JobEventDocument([redacted])")
    }
}
fn present_counter<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<u64>, D::Error> {
    u64::deserialize(d).map(Some)
}
fn present_terminal<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Option<TerminalCorrelation>, D::Error> {
    TerminalCorrelation::deserialize(d).map(Some)
}

/// Language-neutral schema for the complete event document.
pub const SCHEMA: &str = include_str!("../../../schemas/agent-protocol/job/event.json");
