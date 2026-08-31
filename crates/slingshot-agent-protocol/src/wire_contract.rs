//! The order a document is checked in, and why it is that order.
//!
//! Three gates, and each one exists because the next cannot do its job. Raw
//! bytes are checked against the canonical-byte contract first, because a
//! schema cannot see whether the octets it was handed are the canonical
//! spelling of the value they parse to - two different byte strings can parse
//! to one value, and only one of them is the one this system agreed on. Then
//! the decoded shape is checked against the schema, because typed conversion
//! cannot report which field was wrong. Then the typed conversion happens.
//!
//! Running them in any other order produces a system that accepts documents it
//! meant to refuse. A noncanonical document that is shaped correctly would pass
//! a schema and reach typed code, and by then nothing remembers the bytes.

use slingshot_domain::selected_command_contract_identity::SelectedCommandContractIdentity;

use crate::identity::{DocumentProvenance, WireContractIdentity};

/// How far a document has been checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ValidationStage {
    /// The exact octets are the canonical spelling of their value.
    RawCanonicalBytes,
    /// The decoded value is the shape the schema describes.
    DecodedShape,
    /// The value has become the typed thing it claims to be.
    TypedConversion,
}

/// The order those gates run in.
pub const VALIDATION_ORDER: &[ValidationStage] = &[
    ValidationStage::RawCanonicalBytes,
    ValidationStage::DecodedShape,
    ValidationStage::TypedConversion,
];

/// Why a document was refused, and how far it got first.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WireRefusal {
    /// The octets are not the canonical spelling of the value they parse to.
    #[error("these bytes are not the canonical spelling of the value they parse to")]
    NotCanonicalBytes,
    /// The document names another format.
    #[error("this document is written under {named}, and this build speaks {expected}")]
    FormatDrift {
        /// What this build speaks.
        expected: String,
        /// What the document names.
        named: String,
    },
    /// The document names another transport contract.
    #[error("this document names transport contract {named}, and this build has {expected}")]
    TransportContractDrift {
        /// What this build has.
        expected: String,
        /// What the document names.
        named: String,
    },
    /// The document names another canonical-byte contract.
    ///
    /// Reported separately from transport drift, because the two have different
    /// causes and different fixes: one is a disagreement about how to talk, and
    /// the other about what a canonical document looks like.
    #[error("this document names canonical contract {named}, and this build has {expected}")]
    CanonicalContractDrift {
        /// What this build has.
        expected: String,
        /// What the document names.
        named: String,
    },
    /// The document names another command contract.
    #[error("this document names a command contract that is not the installed one")]
    CommandContractDrift,
}

/// What this build expects every document to say.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectedProvenance {
    /// Digest of the canonical-byte contract.
    pub canonical_json_contract_digest: String,
    /// The installed command contract identity.
    pub command_contract: SelectedCommandContractIdentity,
    /// Digest of the transport contract.
    pub transport_contract_digest: String,
}

impl ExpectedProvenance {
    /// Requires `provenance` to name exactly what this build has.
    ///
    /// Checked in one order so a caller reading a refusal learns the first
    /// thing that was wrong rather than whichever the code happened to notice.
    ///
    /// # Errors
    ///
    /// Returns [`WireRefusal`] naming the first field that differs.
    pub fn require_matching(&self, provenance: &DocumentProvenance) -> Result<(), WireRefusal> {
        if provenance.format != crate::identity::AGENT_FORMAT {
            return Err(WireRefusal::FormatDrift {
                expected: crate::identity::AGENT_FORMAT.to_owned(),
                named: provenance.format.clone(),
            });
        }
        if provenance.transport_contract_digest != self.transport_contract_digest {
            return Err(WireRefusal::TransportContractDrift {
                expected: self.transport_contract_digest.clone(),
                named: provenance.transport_contract_digest.clone(),
            });
        }
        if provenance.canonical_json_contract_digest != self.canonical_json_contract_digest {
            return Err(WireRefusal::CanonicalContractDrift {
                expected: self.canonical_json_contract_digest.clone(),
                named: provenance.canonical_json_contract_digest.clone(),
            });
        }
        let named: SelectedCommandContractIdentity = (&provenance.command_contract).into();
        if !self.command_contract.is_the_same_contract_as(&named) {
            return Err(WireRefusal::CommandContractDrift);
        }
        Ok(())
    }

    /// Returns the provenance this build writes on its own documents.
    #[must_use]
    pub fn provenance(&self) -> DocumentProvenance {
        DocumentProvenance {
            canonical_json_contract_digest: self.canonical_json_contract_digest.clone(),
            command_contract: WireContractIdentity::from(&self.command_contract),
            format: crate::identity::AGENT_FORMAT.to_owned(),
            transport_contract_digest: self.transport_contract_digest.clone(),
        }
    }
}
