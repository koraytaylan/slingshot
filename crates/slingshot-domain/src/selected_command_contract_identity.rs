//! The five fields that say exactly which command contract a submission means.
//!
//! A command is not identified by its name. Two builds can both call something
//! `query_paths` and disagree about what its arguments are, what its result
//! looks like, or how large either may be - and an agent that accepted a
//! submission on the strength of the name alone would run the wrong contract
//! against a remote system. So an identity is five fields: the wire name, the
//! semantic version, the digest of the limits, and the digests of both role
//! schemas. All five, unchanged, or the submission is refused.
//!
//! The canonical-byte contract is authenticated separately, and the order
//! matters. A role schema carries an annotation naming the canonical contract
//! it was written under; that annotation is checked against the digest of the
//! committed contract bytes before the role digest is believed at all. So
//! contract drift and annotation drift are two different failures with two
//! different causes, and neither can hide inside the other.
//!
//! Nothing here proves canonicality of bytes. A schema describes shape; whether
//! a document's bytes are the canonical spelling of that shape is a separate
//! question, answered before a schema is consulted. Conflating the two would
//! let a noncanonical document through on the strength of being shaped right.

use sha2::{Digest as _, Sha256};

use crate::command::command_identity::{CommandContract, INITIAL_COMMAND_VERSION};
use crate::command::schema::{
    CANONICAL_CONTRACT_ANNOTATION, SchemaRole, canonical_contract_digest, command_schema,
    schema_manifest,
};

/// Separator between the fields a submitted-command digest is taken over.
pub const FIELD_SEPARATOR: u8 = 0;

/// Version marker every submitted-command digest is derived under.
pub const SUBMITTED_COMMAND_DIGEST_VERSION: &str = "slingshot.submitted-command/1";

/// Reason a command contract identity could not be established.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ContractIdentityFailure {
    /// The registry holds no command under that wire name.
    #[error("no command is registered under the wire name {wire_name}")]
    UnknownCommand {
        /// The name that was asked for.
        wire_name: String,
    },
    /// A role schema names a canonical contract this build does not have.
    ///
    /// Reported separately from every other drift, because it has a different
    /// cause: the schema was written under a byte contract that is not the one
    /// installed here, and nothing about its shape reveals that.
    #[error(
        "the {role} schema of {wire_name} names canonical contract {named}, and this build has {installed}"
    )]
    CanonicalContractAnnotationDrift {
        /// What this build's contract digests to.
        installed: String,
        /// What the schema says it was written under.
        named: String,
        /// Which role schema.
        role: &'static str,
        /// Which command.
        wire_name: String,
    },
    /// A role schema carries no canonical-contract annotation at all.
    #[error("the {role} schema of {wire_name} carries no canonical-contract annotation")]
    CanonicalContractAnnotationAbsent {
        /// Which role schema.
        role: &'static str,
        /// Which command.
        wire_name: String,
    },
}

/// Exactly which command contract one submission means.
///
/// Every field participates. A submission whose identity differs from the
/// agent's in any one of them is refused, because each of the five can change
/// what running the command actually does.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SelectedCommandContractIdentity {
    /// Digest of the argument schema.
    pub argument_schema_digest: String,
    /// Digest of the limits manifest.
    pub command_contract_limits_digest: String,
    /// The semantic version this contract is at.
    pub command_semantic_contract_version: String,
    /// The name this command answers to on the wire.
    pub command_wire_name: String,
    /// Digest of the result schema.
    pub result_schema_digest: String,
}

impl SelectedCommandContractIdentity {
    /// Returns the identity the installed registry gives `wire_name`.
    ///
    /// Both role schemas are checked against the installed canonical contract
    /// before their digests are read, so an identity is never assembled from a
    /// schema written under a contract this build does not have.
    ///
    /// # Errors
    ///
    /// Returns [`ContractIdentityFailure::UnknownCommand`],
    /// [`ContractIdentityFailure::CanonicalContractAnnotationAbsent`], or
    /// [`ContractIdentityFailure::CanonicalContractAnnotationDrift`].
    pub fn installed(wire_name: &str) -> Result<Self, ContractIdentityFailure> {
        let manifest = schema_manifest();
        let installed = canonical_contract_digest();
        let digests = manifest["schemas"].get(wire_name).ok_or_else(|| {
            ContractIdentityFailure::UnknownCommand { wire_name: wire_name.to_owned() }
        })?;
        for role in SchemaRole::both() {
            require_annotation(wire_name, role, &installed)?;
        }
        Ok(Self {
            argument_schema_digest: role_digest(digests, SchemaRole::Arguments),
            command_contract_limits_digest: manifest["command_contract_limits_sha256"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
            command_semantic_contract_version: INITIAL_COMMAND_VERSION.to_owned(),
            command_wire_name: wire_name.to_owned(),
            result_schema_digest: role_digest(digests, SchemaRole::Result),
        })
    }

    /// Returns whether this identity is the same contract as `other`.
    ///
    /// Spelled out rather than left to equality so a reader can see there is no
    /// partial match: five fields, and all of them.
    #[must_use]
    pub fn is_the_same_contract_as(&self, other: &Self) -> bool {
        self.argument_schema_digest == other.argument_schema_digest
            && self.command_contract_limits_digest == other.command_contract_limits_digest
            && self.command_semantic_contract_version == other.command_semantic_contract_version
            && self.command_wire_name == other.command_wire_name
            && self.result_schema_digest == other.result_schema_digest
    }
}

/// Returns one role's digest from a manifest entry.
fn role_digest(digests: &serde_json::Value, role: SchemaRole) -> String {
    digests[role.as_text()].as_str().unwrap_or_default().to_owned()
}

/// Requires one role schema to name the canonical contract this build has.
fn require_annotation(
    wire_name: &str,
    role: SchemaRole,
    installed: &str,
) -> Result<(), ContractIdentityFailure> {
    let schema = command_schema(wire_name, role);
    let named = schema.get(CANONICAL_CONTRACT_ANNOTATION).and_then(serde_json::Value::as_str);
    match named {
        None => Err(ContractIdentityFailure::CanonicalContractAnnotationAbsent {
            role: role.as_text(),
            wire_name: wire_name.to_owned(),
        }),
        Some(named) if named != installed => {
            Err(ContractIdentityFailure::CanonicalContractAnnotationDrift {
                installed: installed.to_owned(),
                named: named.to_owned(),
                role: role.as_text(),
                wire_name: wire_name.to_owned(),
            })
        }
        Some(_) => Ok(()),
    }
}

/// The digest one submission is bound to, over everything that decides it.
///
/// The canonical-contract digest sits between the limits and the role schemas,
/// which is not decoration: it means a submission binds the byte contract as a
/// peer of the shapes rather than as something the shapes happen to mention. A
/// build that changed the byte contract without changing a schema produces a
/// different submitted digest, and an agent notices.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SubmittedCommandDigest {
    /// The digest, in lowercase hexadecimal.
    spelling: String,
}

impl SubmittedCommandDigest {
    /// Returns the digest one submission of `canonical_arguments` produces.
    #[must_use]
    pub fn derive(
        identity: &SelectedCommandContractIdentity,
        canonical_contract_digest: &str,
        transport_contract_digest: &str,
        canonical_arguments: &str,
    ) -> Self {
        let mut hasher = Sha256::new();
        for field in [
            SUBMITTED_COMMAND_DIGEST_VERSION,
            transport_contract_digest,
            &identity.command_wire_name,
            &identity.command_semantic_contract_version,
            &identity.command_contract_limits_digest,
            canonical_contract_digest,
            &identity.argument_schema_digest,
            &identity.result_schema_digest,
            canonical_arguments,
        ] {
            hasher.update(field.as_bytes());
            hasher.update([FIELD_SEPARATOR]);
        }
        Self { spelling: hasher.finalize().iter().map(|octet| format!("{octet:02x}")).collect() }
    }

    /// Returns this digest's spelling.
    #[must_use]
    pub fn as_text(&self) -> &str {
        &self.spelling
    }
}

/// Returns the limits digest the installed registry has.
///
/// Read through the same manifest an identity is built from, so the two cannot
/// disagree about what this build's limits are.
#[must_use]
pub fn installed_limits_digest() -> String {
    schema_manifest()["command_contract_limits_sha256"].as_str().unwrap_or_default().to_owned()
}

/// Returns whether the installed limits are the ones `digest` names.
#[must_use]
pub fn limits_are_installed(digest: &str) -> bool {
    installed_limits_digest() == digest
        && CommandContract::embedded().limit("maximum_command_wire_name_bytes") > 0
}
