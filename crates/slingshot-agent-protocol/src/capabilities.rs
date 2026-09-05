//! The closed capability handshake preceding command derivation and recovery.

use crate::identity::WireContractIdentity;
use serde::{Deserialize, Serialize};

/// Wire shape only; consumers validate bounds, uniqueness and compatibility.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capabilities {
    /// Versioned document format.
    pub format: String,
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
