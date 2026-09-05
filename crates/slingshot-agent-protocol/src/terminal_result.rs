//! Shared terminal-result envelope. Decoding this type is not evidence of
//! success: the consumer must check retained identity, provenance, canonical
//! bytes, command schema, request correlation and artifact ownership.

use crate::identity::{DocumentProvenance, WireOperationIdentity};

/// Metadata echoed for one artifact declared by the typed command result.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactEcho {
    /// Length of the identity-encoded content.
    pub byte_length: u64,
    /// Parameter-free media type.
    pub media_type: String,
    /// The command's declared artifact slot.
    pub slot: String,
    /// Suggested download filename, not a local path.
    pub suggested_name: String,
}

/// Untrusted terminal result received from the author.
#[derive(Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct TerminalResultDocument {
    /// Exact retained generation, operation, target and selected revision.
    pub operation: WireOperationIdentity,
    /// Subscription registered by the retained submission.
    pub daemon_subscription_identifier: String,
    /// Canonical JSON as a string, preserving the result's exact bytes.
    pub canonical_result: String,
    /// Metadata that must exactly match the typed result's artifacts.
    pub declared_artifacts: Vec<ArtifactEcho>,
    /// Versioned transport and selected command contracts.
    pub provenance: DocumentProvenance,
    /// Digest of the retained command and artifact manifest.
    pub submitted_command_digest: String,
}

impl core::fmt::Debug for TerminalResultDocument {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("TerminalResultDocument([redacted])")
    }
}

/// Published closed envelope schema; semantic validation remains mandatory.
pub const SCHEMA: &str = include_str!("../../../schemas/agent-protocol/job/terminal-result.json");
