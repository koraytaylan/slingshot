//! Finding out what an agent is before asking it to do anything.
//!
//! Discovery exists so that a disagreement is found while it is still cheap. A
//! daemon that submitted first and discovered afterwards that the agent speaks
//! a different transport, holds a different command contract, or has rebuilt
//! its event store would have created work it cannot follow - and would have
//! done it against a remote system that may already be running it.
//!
//! So capabilities are compared before a reservation, before a submission, and
//! before anything is written down. Each thing that can differ has its own
//! refusal, because the fixes are different: a transport disagreement is a
//! version problem, a command-contract disagreement is a build problem, and a
//! generation change is a store that was rebuilt underneath a daemon that has
//! rows referring to it.
//!
//! Readiness is part of what is compared. An agent whose continuation-key
//! authority is not ready cannot issue tokens that will still validate, and
//! finding that out after a paged query has begun is finding it out too late.

use slingshot_agent_protocol::identity::WireContractIdentity;
use slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract;
use slingshot_domain::selected_command_contract_identity::SelectedCommandContractIdentity;

/// What an agent says it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdvertisedCapabilities {
    /// Which incarnation of its event store it is serving.
    pub agent_event_store_generation: u64,
    /// The canonical-byte contract its schemas were written under.
    pub canonical_json_contract_digest: String,
    /// The command contracts it holds, in wire order.
    pub command_contracts: Vec<WireContractIdentity>,
    /// Whether its continuation-key authority can issue and validate.
    pub continuation_authority_ready: bool,
    /// The transport contract it speaks.
    pub transport_contract_digest: String,
}

/// What this daemon requires an agent to be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequiredCapabilities {
    /// The canonical-byte contract this build has.
    pub canonical_json_contract_digest: String,
    /// The command contract this daemon is about to use.
    pub command_contract: SelectedCommandContractIdentity,
    /// The generation this daemon's rows refer to, once it has any.
    pub expected_generation: Option<u64>,
    /// The transport contract this build speaks.
    pub transport_contract_digest: String,
}

/// Why an agent is not one this daemon may use.
///
/// One variant per thing that can differ, because the fixes are different and
/// a single "incompatible" would leave an operator to work out which.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DiscoveryRefusal {
    /// The agent speaks another transport contract.
    #[error("this agent speaks transport contract {advertised}, and this build speaks {required}")]
    TransportContractIncompatible {
        /// What the agent advertises.
        advertised: String,
        /// What this build speaks.
        required: String,
    },
    /// The agent's schemas were written under another canonical contract.
    #[error(
        "this agent's schemas name canonical contract {advertised}, and this build has {required}"
    )]
    CanonicalContractIncompatible {
        /// What the agent advertises.
        advertised: String,
        /// What this build has.
        required: String,
    },
    /// The agent holds no matching command contract.
    #[error("this agent holds no contract matching {command_wire_name} as this build has it")]
    CommandContractAbsent {
        /// Which command was looked for.
        command_wire_name: String,
    },
    /// The agent's continuation-key authority is not ready.
    #[error("this agent's continuation-key authority is not ready, so its tokens would not last")]
    ContinuationAuthorityNotReady,
    /// The agent's event store was rebuilt.
    #[error(
        "this agent serves generation {advertised}, and this daemon holds rows from {expected}"
    )]
    GenerationChanged {
        /// What the agent serves now.
        advertised: u64,
        /// What this daemon's rows refer to.
        expected: u64,
    },
}

impl RequiredCapabilities {
    /// Requires `advertised` to be an agent this daemon may use.
    ///
    /// Ordered from the most fundamental disagreement outwards. Transport
    /// first, because two sides that cannot agree on how to talk have nothing
    /// to say about what they hold; then the canonical contract, because it
    /// decides what a well-formed document even is; then the command contract,
    /// readiness, and the generation.
    ///
    /// # Errors
    ///
    /// Returns [`DiscoveryRefusal`] naming the first thing that differs.
    pub fn require_compatible(
        &self,
        advertised: &AdvertisedCapabilities,
    ) -> Result<(), DiscoveryRefusal> {
        if advertised.transport_contract_digest != self.transport_contract_digest {
            return Err(DiscoveryRefusal::TransportContractIncompatible {
                advertised: advertised.transport_contract_digest.clone(),
                required: self.transport_contract_digest.clone(),
            });
        }
        if advertised.canonical_json_contract_digest != self.canonical_json_contract_digest {
            return Err(DiscoveryRefusal::CanonicalContractIncompatible {
                advertised: advertised.canonical_json_contract_digest.clone(),
                required: self.canonical_json_contract_digest.clone(),
            });
        }
        let holds = advertised.command_contracts.iter().any(|held| {
            SelectedCommandContractIdentity::from(held)
                .is_the_same_contract_as(&self.command_contract)
        });
        if !holds {
            return Err(DiscoveryRefusal::CommandContractAbsent {
                command_wire_name: self.command_contract.command_wire_name.clone(),
            });
        }
        if !advertised.continuation_authority_ready {
            return Err(DiscoveryRefusal::ContinuationAuthorityNotReady);
        }
        if let Some(expected) = self.expected_generation
            && advertised.agent_event_store_generation != expected
        {
            return Err(DiscoveryRefusal::GenerationChanged {
                advertised: advertised.agent_event_store_generation,
                expected,
            });
        }
        Ok(())
    }

    /// Returns what this build requires, for `command_contract`.
    #[must_use]
    pub fn of(
        command_contract: SelectedCommandContractIdentity,
        canonical_json_contract_digest: &str,
        expected_generation: Option<u64>,
    ) -> Self {
        Self {
            canonical_json_contract_digest: canonical_json_contract_digest.to_owned(),
            command_contract,
            expected_generation,
            transport_contract_digest: AuthorAgentTransportContract::embedded_digest(),
        }
    }
}
