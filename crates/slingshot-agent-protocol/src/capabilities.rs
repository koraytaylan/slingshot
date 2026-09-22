//! The closed capability handshake preceding command derivation and recovery.

use crate::identity::WireContractIdentity;
use serde::{Deserialize, Serialize};

/// The capability revision this build requires an agent to declare.
///
/// Format and contract digests answer "can these two speak at all". They do not
/// answer "was the agent built with the behaviour this client expects", because
/// a behavioural fix that changes no contract leaves every digest identical. An
/// agent deployed before such a fix describes the same format and the same
/// contracts while answering differently, and a client that could not tell them
/// apart would read the older build's refusal as an unreadable outcome.
///
/// So the agent declares a revision, and this constant is what this client
/// requires. The revision stays at zero.
/// It is deliberately separate from `format`: a format names the document
/// shape, and this names the build's behaviour.
pub const REQUIRED_CAPABILITY_REVISION: u64 = 0;

/// Wire shape only; consumers validate bounds, uniqueness and compatibility.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capabilities {
    /// Versioned document format.
    pub format: String,
    /// Which behavioural revision this agent was built with.
    pub capability_revision: u64,
    /// Current event-store incarnation.
    pub agent_event_store_generation: u64,
    /// Canonical-byte contract digest.
    pub canonical_json_contract_digest: String,
    /// Advertised five-field command identities.
    pub command_contracts: Vec<WireContractIdentity>,
    /// Whether the continuation authority is ready.
    pub continuation_authority_ready: bool,
    /// Exact transport contract digest.
    pub transport_contract_digest: String,
}

impl core::fmt::Debug for Capabilities {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("Capabilities([redacted])")
    }
}
