//! Every number the author transport is held to, in one place with a digest.
//!
//! A manifest rather than constants scattered through the code, for one
//! reason: the far side has to agree. A daemon and an agent that each held
//! their own idea of how large a header may be would disagree silently, at the
//! worst moment, about a request that looked fine to whichever of them wrote
//! it. So the numbers live in bytes that can be digested, both sides advertise
//! the digest, and a mismatch is a refusal before anything is reserved.
//!
//! There is no deployment override and no fallback. A value this build cannot
//! find is a build that refuses to construct, not a build that picks something
//! reasonable - because "reasonable" is exactly what two implementations
//! disagree about.
//!
//! The formulas are checked rather than trusted. Each one is recomputed from
//! its operands in unsigned arithmetic, so a manifest whose stated result does
//! not follow from its own inputs fails here rather than somewhere downstream
//! that assumed it followed.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::Deserialize;
use sha2::{Digest as _, Sha256};

/// Format the committed contract is written under.
pub const CONTRACT_FORMAT: &str = "slingshot.author-agent-transport-contract/1";

/// The protocol version this build speaks.
pub const AGENT_PROTOCOL_VERSION: u64 = 1;

/// Whole events one server-sent-event buffer holds.
const EVENTS_PER_MAXIMUM_BUFFER: u64 = 2;

/// Heartbeat intervals one heartbeat timeout allows.
const HEARTBEATS_PER_TIMEOUT: u64 = 3;

/// Artifacts one reserved operation may produce.
const ARTIFACTS_PER_RESERVATION: u64 = 2;

/// Halves a lease is compared against, so renewal happens well inside it.
const LEASE_HALVES: u64 = 2;

/// Reason the transport contract could not be established.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TransportContractFailure {
    /// The committed bytes are not one readable document.
    #[error("the committed transport contract is not readable: {0}")]
    Unreadable(String),
    /// The document names another format.
    #[error("the committed transport contract names {0}, and this build speaks {CONTRACT_FORMAT}")]
    UnsupportedFormat(String),
    /// The sidecar does not digest the committed bytes.
    #[error("the sidecar names {sidecar}, and the committed bytes digest to {computed}")]
    DigestMismatch {
        /// What the bytes digest to.
        computed: String,
        /// What the sidecar says.
        sidecar: String,
    },
    /// A formula's stated result does not follow from its own operands.
    #[error("the formula {name} does not follow from its operands")]
    FormulaInconsistent {
        /// Which formula.
        name: String,
    },
    /// A value this build needs is not in the contract.
    ///
    /// Refused rather than defaulted. A build that picked something reasonable
    /// would be a build that disagrees with the far side about what reasonable
    /// means, silently.
    #[error("the transport contract names no {name}, and this build has no fallback for one")]
    ValueAbsent {
        /// Which value.
        name: String,
    },
}

/// Every number the author transport is held to.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorAgentTransportContract {
    /// The protocol version the contract is for.
    pub agent_protocol_version: u64,
    /// The format it is written under.
    pub format: String,
    /// Values computed from other values.
    pub formulas: BTreeMap<String, u64>,
    /// Values stated outright.
    pub limits: BTreeMap<String, u64>,
}

/// The committed contract bytes, embedded at compile time.
const COMMITTED: &str = include_str!("../../../policy/author-agent-transport-contract-1.json");

/// The committed sidecar, embedded at compile time.
const SIDECAR: &str = include_str!("../../../policy/author-agent-transport-contract-1.sha256");

/// The parsed contract, read once.
static EMBEDDED: OnceLock<AuthorAgentTransportContract> = OnceLock::new();

impl AuthorAgentTransportContract {
    /// Returns the contract this build was compiled with.
    ///
    /// # Panics
    ///
    /// Panics when the committed bytes are not a consistent contract, which is
    /// a defect in this repository rather than in any input.
    #[must_use]
    pub fn embedded() -> &'static Self {
        EMBEDDED.get_or_init(|| {
            let contract = Self::parse(COMMITTED).expect("the committed contract parses");
            contract.require_sidecar(SIDECAR).expect("the committed sidecar matches");
            contract.require_consistent_formulas().expect("the committed formulas follow");
            contract
        })
    }

    /// Returns the committed bytes, exactly as they are on disk.
    #[must_use]
    pub fn embedded_manifest() -> &'static str {
        COMMITTED
    }

    /// Returns the digest both sides advertise.
    #[must_use]
    pub fn embedded_digest() -> String {
        render(&Sha256::digest(COMMITTED.as_bytes()))
    }

    /// Returns the contract `text` spells.
    ///
    /// # Errors
    ///
    /// Returns [`TransportContractFailure::Unreadable`] or
    /// [`TransportContractFailure::UnsupportedFormat`].
    pub fn parse(text: &str) -> Result<Self, TransportContractFailure> {
        let contract: Self = serde_json::from_str(text)
            .map_err(|failure| TransportContractFailure::Unreadable(failure.to_string()))?;
        if contract.format != CONTRACT_FORMAT {
            return Err(TransportContractFailure::UnsupportedFormat(contract.format));
        }
        Ok(contract)
    }

    /// Requires `sidecar` to be the digest of the committed bytes.
    ///
    /// # Errors
    ///
    /// Returns [`TransportContractFailure::DigestMismatch`].
    pub fn require_sidecar(&self, sidecar: &str) -> Result<(), TransportContractFailure> {
        let computed = Self::embedded_digest();
        let named = sidecar.trim();
        if named == computed {
            Ok(())
        } else {
            Err(TransportContractFailure::DigestMismatch { computed, sidecar: named.to_owned() })
        }
    }

    /// Returns the limit `name` states.
    ///
    /// # Panics
    ///
    /// Panics when the contract names no such limit, because a build without
    /// one has no reasonable thing to do instead.
    #[must_use]
    pub fn limit(&self, name: &str) -> u64 {
        *self.limits.get(name).unwrap_or_else(|| {
            panic!("the transport contract names no {name}, and this build has no fallback")
        })
    }

    /// Returns the formula `name` states.
    ///
    /// # Panics
    ///
    /// Panics when the contract names no such formula.
    #[must_use]
    pub fn formula(&self, name: &str) -> u64 {
        *self.formulas.get(name).unwrap_or_else(|| {
            panic!("the transport contract names no {name}, and this build has no fallback")
        })
    }

    /// Returns the limit `name` states, or reports that it is absent.
    ///
    /// # Errors
    ///
    /// Returns [`TransportContractFailure::ValueAbsent`].
    pub fn require_limit(&self, name: &str) -> Result<u64, TransportContractFailure> {
        self.limits
            .get(name)
            .copied()
            .ok_or_else(|| TransportContractFailure::ValueAbsent { name: name.to_owned() })
    }

    /// Requires every formula to follow from its own operands.
    ///
    /// Recomputed rather than trusted, in unsigned arithmetic, so a manifest
    /// whose stated result does not follow fails here rather than downstream
    /// where something assumed it did.
    ///
    /// # Errors
    ///
    /// Returns [`TransportContractFailure::FormulaInconsistent`] naming the
    /// first formula that does not follow.
    pub fn require_consistent_formulas(&self) -> Result<(), TransportContractFailure> {
        let reservation = self.limit("maximum_current_generation_operation_reservation_rows");
        let expected = [
            (
                "maximum_server_sent_event_buffer_bytes",
                self.limit("maximum_server_sent_event_bytes")
                    .checked_mul(EVENTS_PER_MAXIMUM_BUFFER),
            ),
            (
                "heartbeat_timeout_milliseconds",
                self.limit("heartbeat_interval_milliseconds").checked_mul(HEARTBEATS_PER_TIMEOUT),
            ),
            (
                "maximum_current_generation_event_rows",
                reservation.checked_mul(self.limit("maximum_operation_event_rows")),
            ),
            (
                "maximum_current_generation_artifact_rows",
                reservation.checked_mul(ARTIFACTS_PER_RESERVATION),
            ),
        ];
        for (name, produced) in expected {
            if produced != Some(self.formula(name)) {
                return Err(TransportContractFailure::FormulaInconsistent {
                    name: name.to_owned(),
                });
            }
        }
        self.require_consistent_relations()
    }

    /// Requires the relations between limits that are not multiplications.
    ///
    /// Separate from the products because they are a different kind of claim:
    /// not "this equals that times something" but "this has to be smaller than
    /// that or the design does not work".
    fn require_consistent_relations(&self) -> Result<(), TransportContractFailure> {
        let renewal = self.limit("worker_execution_lease_renewal_milliseconds");
        let lease = self.limit("worker_execution_lease_milliseconds");
        if renewal.checked_mul(LEASE_HALVES).is_none_or(|doubled| doubled >= lease) {
            return Err(TransportContractFailure::FormulaInconsistent {
                name: "worker_execution_lease_renewal_milliseconds".to_owned(),
            });
        }
        let submission = self.limit("maximum_canonical_submission_bytes");
        let fits = submission < self.limit("maximum_agent_protocol_document_bytes")
            && submission < self.limit("maximum_finite_response_body_bytes");
        if !fits {
            return Err(TransportContractFailure::FormulaInconsistent {
                name: "maximum_canonical_submission_bytes".to_owned(),
            });
        }
        Ok(())
    }
}

/// Returns `octets` in lowercase hexadecimal.
fn render(octets: &[u8]) -> String {
    octets.iter().map(|octet| format!("{octet:02x}")).collect()
}
