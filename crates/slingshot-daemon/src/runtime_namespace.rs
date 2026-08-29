//! Runtime namespace derivation.
//!
//! One `(profile, environment)` pair names one runtime namespace. The namespace
//! digest is taken over a length-prefixed encoding of both names, so two pairs
//! that would read the same when joined by a delimiter still produce different
//! namespaces. Display values stay separate from the platform identifiers built
//! from the digest, so a diagnostic can never be mistaken for an endpoint.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use slingshot_local_protocol::foundation_contract::FoundationContract;

/// Domain separator every namespace digest is taken over.
pub const NAMESPACE_DIGEST_DOMAIN: &[u8] = b"slingshot.runtime-namespace/1";

/// Reason a target could not name a runtime namespace.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NamespaceFailure {
    /// A target name is empty.
    #[error("the {name} is empty")]
    Empty {
        /// Which of the two names is empty.
        name: &'static str,
    },
    /// A target name is beyond the bound the foundation contract declares.
    #[error("the {name} is {length} bytes, beyond the limit of {limit}")]
    TooLong {
        /// Which of the two names is too long.
        name: &'static str,
        /// Length the name reached.
        length: usize,
        /// Largest length the contract allows.
        limit: usize,
    },
    /// A target name carries a character a namespace cannot be built from.
    #[error("the {name} carries the character {character:?}, which a namespace cannot use")]
    Unusable {
        /// Which of the two names is unusable.
        name: &'static str,
        /// First character that cannot be used.
        character: char,
    },
}

/// One runtime namespace, named by a profile and an environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeNamespace {
    profile: String,
    environment: String,
    digest: String,
    runtime_root: PathBuf,
}

/// Reports the first character of a target name a namespace cannot be built from.
fn unusable_character(name: &str) -> Option<char> {
    name.chars().find(|character| {
        character.is_control() || *character == '/' || *character == '\\' || *character == '\0'
    })
}

/// Refuses a target name that is empty, too long, or unusable.
fn evaluate_name(label: &'static str, value: &str, limit: u32) -> Result<(), NamespaceFailure> {
    if value.is_empty() {
        return Err(NamespaceFailure::Empty { name: label });
    }
    if value.len() > limit as usize {
        return Err(NamespaceFailure::TooLong {
            name: label,
            length: value.len(),
            limit: limit as usize,
        });
    }
    match unusable_character(value) {
        Some(character) => Err(NamespaceFailure::Unusable { name: label, character }),
        None => Ok(()),
    }
}

/// Appends one length-prefixed name to a digest.
fn absorb(digest: &mut Sha256, value: &str) {
    let length = u32::try_from(value.len()).unwrap_or(u32::MAX);
    digest.update(length.to_be_bytes());
    digest.update(value.as_bytes());
}

impl RuntimeNamespace {
    /// Names the runtime namespace of one target inside `runtime_root`.
    ///
    /// # Errors
    ///
    /// Returns [`NamespaceFailure`] when either name is empty, beyond the bound
    /// the foundation contract declares, or carries an unusable character.
    pub fn name(
        contract: &FoundationContract,
        runtime_root: &Path,
        profile: &str,
        environment: &str,
    ) -> Result<Self, NamespaceFailure> {
        evaluate_name("profile", profile, contract.names.profile_bytes)?;
        evaluate_name("environment", environment, contract.names.environment_bytes)?;
        let mut digest = Sha256::new();
        digest.update(NAMESPACE_DIGEST_DOMAIN);
        absorb(&mut digest, profile);
        absorb(&mut digest, environment);
        Ok(Self {
            profile: profile.to_owned(),
            environment: environment.to_owned(),
            digest: hex::encode(digest.finalize()),
            runtime_root: runtime_root.to_path_buf(),
        })
    }

    /// Returns the profile this namespace belongs to.
    #[must_use]
    pub fn profile(&self) -> &str {
        &self.profile
    }

    /// Returns the environment this namespace belongs to.
    #[must_use]
    pub fn environment(&self) -> &str {
        &self.environment
    }

    /// Returns the namespace digest, rendered in lowercase hexadecimal.
    #[must_use]
    pub fn digest(&self) -> &str {
        &self.digest
    }

    /// Returns the runtime root this namespace's objects live in.
    #[must_use]
    pub fn runtime_root(&self) -> &Path {
        &self.runtime_root
    }

    /// Returns the value a diagnostic shows for this namespace.
    ///
    /// The display value is never an endpoint name and never a lock identity.
    #[must_use]
    pub fn display(&self) -> String {
        format!("{}/{}", self.profile, self.environment)
    }
}
