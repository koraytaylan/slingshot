//! Closed artifact refusal, bound to the retained command and artifact identity.
use crate::identity::DocumentProvenance;
use serde::{Deserialize, Serialize};

/// An unavailable response carries identity, never an alternate download URL.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactUnavailable {
    /// Exact retained format and command/transport provenance.
    pub provenance: DocumentProvenance,
    /// Agent store generation that was consulted.
    pub agent_event_store_generation: u64,
    /// The derived remote logical operation.
    pub agent_operation_identifier: String,
    /// The deterministic artifact identity, not its content digest.
    pub artifact_identifier: String,
    /// The command-declared remote slot.
    pub artifact_slot: String,
    /// Closed cause, also constrained by the HTTP status.
    pub reason: UnavailableReason,
}

impl core::fmt::Debug for ArtifactUnavailable {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("ArtifactUnavailable([redacted])")
    }
}

/// Missing may be propagation delay; explicit retirement ends acquisition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnavailableReason {
    /// HTTP 404.
    Missing,
    /// HTTP 410.
    RetentionExpired,
}

/// Published closed schema for this response.
pub const SCHEMA: &str =
    include_str!("../../../schemas/agent-protocol/job/artifact-unavailable.json");
