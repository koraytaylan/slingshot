//! Untrusted terminal-failure envelope. Consumers must authenticate retained
//! provenance and identity, then validate exact canonical failure bytes against
//! the selected command's closed refusal type and request correlation rules.

use crate::identity::{DocumentProvenance, WireOperationIdentity};

/// The complete remote failure document, not evidence of its effect disposition.
#[derive(Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct TerminalFailureDocument {
    /// Exact retained operation/generation/target/revision partition.
    pub operation: WireOperationIdentity,
    /// Subscription registered with the retained submission.
    pub daemon_subscription_identifier: String,
    /// Exact failure bytes, not a decoded map that could repair noncanonical input.
    pub canonical_failure: String,
    /// Installed transport, canonical and selected command contracts.
    pub provenance: DocumentProvenance,
    /// Digest binding the retained command and artifact manifest.
    pub submitted_command_digest: String,
}

impl core::fmt::Debug for TerminalFailureDocument {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("TerminalFailureDocument([redacted])")
    }
}

/// Closed language-neutral envelope schema; semantic validation is separate.
pub const SCHEMA: &str = include_str!("../../../schemas/agent-protocol/job/terminal-failure.json");
