//! Identity as it crosses the wire to the agent.
//!
//! Every operation-bearing document carries two digests that are not the same
//! thing and must not be confused: the transport contract this daemon and agent
//! agreed on, and the canonical-byte contract the command schemas were written
//! under. One says how to talk; the other says what a canonical document looks
//! like. A build that changed either without the other produces documents the
//! far side refuses, which is the point.
//!
//! Nothing in a schema claims that bytes are canonical. Canonicality is a
//! property of the exact octets, checked before any schema is consulted, and a
//! schema that asserted it would be asserting something it cannot see.

use serde::{Deserialize, Serialize};
use slingshot_domain::agent_identity::{AgentEventStoreGeneration, AgentOperationIdentifier};
use slingshot_domain::selected_command_contract_identity::SelectedCommandContractIdentity;

/// Format every agent document is written under.
pub const AGENT_FORMAT: &str = "slingshot.agent/1";

/// What every operation-bearing document says about which contracts it means.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocumentProvenance {
    /// Digest of the canonical-byte contract the schemas were written under.
    pub canonical_json_contract_digest: String,
    /// The five fields naming exactly which command contract this is.
    pub command_contract: WireContractIdentity,
    /// The format this document is written under.
    pub format: String,
    /// Digest of the transport contract this daemon and agent agreed on.
    pub transport_contract_digest: String,
}

/// The five-field identity, as it is written on the wire.
///
/// A separate type from the domain's, because a wire document is a thing that
/// may be malformed and the domain value is a thing that is not. Converting
/// between them is where a document stops being bytes somebody sent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WireContractIdentity {
    /// Digest of the argument schema.
    pub argument_schema_digest: String,
    /// Digest of the limits manifest.
    pub command_contract_limits_digest: String,
    /// The semantic version this contract is at.
    pub command_semantic_contract_version: String,
    /// The name this command answers to.
    pub command_wire_name: String,
    /// Digest of the result schema.
    pub result_schema_digest: String,
}

impl From<&SelectedCommandContractIdentity> for WireContractIdentity {
    fn from(identity: &SelectedCommandContractIdentity) -> Self {
        Self {
            argument_schema_digest: identity.argument_schema_digest.clone(),
            command_contract_limits_digest: identity.command_contract_limits_digest.clone(),
            command_semantic_contract_version: identity.command_semantic_contract_version.clone(),
            command_wire_name: identity.command_wire_name.clone(),
            result_schema_digest: identity.result_schema_digest.clone(),
        }
    }
}

impl From<&WireContractIdentity> for SelectedCommandContractIdentity {
    fn from(identity: &WireContractIdentity) -> Self {
        Self {
            argument_schema_digest: identity.argument_schema_digest.clone(),
            command_contract_limits_digest: identity.command_contract_limits_digest.clone(),
            command_semantic_contract_version: identity.command_semantic_contract_version.clone(),
            command_wire_name: identity.command_wire_name.clone(),
            result_schema_digest: identity.result_schema_digest.clone(),
        }
    }
}

/// Which operation at which incarnation of the agent's store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WireOperationIdentity {
    /// Which incarnation of the store this belongs to.
    pub agent_event_store_generation: u64,
    /// What this operation is called at the agent.
    pub agent_operation_identifier: String,
    /// The partition it belongs to.
    pub author_target_identity_digest: String,
    /// The environment revision it was submitted under.
    pub selected_environment_revision: String,
}

impl WireOperationIdentity {
    /// Returns the identity one operation has at the agent.
    #[must_use]
    pub fn of(
        author_target_identity_digest: &str,
        selected_environment_revision: &str,
        operation_identifier: &str,
        generation: AgentEventStoreGeneration,
    ) -> Self {
        Self {
            agent_event_store_generation: generation.value(),
            agent_operation_identifier: AgentOperationIdentifier::derive(
                author_target_identity_digest,
                selected_environment_revision,
                operation_identifier,
                generation,
            )
            .as_text()
            .to_owned(),
            author_target_identity_digest: author_target_identity_digest.to_owned(),
            selected_environment_revision: selected_environment_revision.to_owned(),
        }
    }
}
