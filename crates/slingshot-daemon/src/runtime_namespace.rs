//! Runtime namespace derivation, over two independent roots.
//!
//! One `(profile, environment)` pair names one runtime namespace. The namespace
//! digest is taken over a length-prefixed encoding of both names, so two pairs
//! that would read the same when joined by a delimiter still produce different
//! namespaces. Display values stay separate from the platform identifiers built
//! from the digest, so a diagnostic can never be mistaken for an endpoint.
//!
//! Only the two names reach the digest. Nothing about who the target
//! authenticates as does - not the author address, not the selected environment
//! revision, not the principal. That is deliberate: rotating a secret or
//! selecting another security context must not move a daemon to a different
//! endpoint, because the process that owns a profile and environment owns it
//! whoever it turns out to be talking to. What those values do partition is the
//! durable data, and that partitioning is the author-target digest's job, one
//! layer down.
//!
//! The two roots are independent because they answer to different lifetimes.
//! Endpoints, locks, and readiness records belong to a login session and are
//! expected to vanish with it; databases and artifacts are the work itself and
//! must not. Replacing the runtime root is a new login, and everything durable
//! is still there afterwards.
//!
//! A name reaches a path only after being escaped down to characters a path
//! component may hold, and the digest is appended to whatever survives. So the
//! readable part is a convenience for whoever is reading a directory listing,
//! and the digest is what actually distinguishes two namespaces - including two
//! whose names escape to the same readable text.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use slingshot_local_protocol::foundation_contract::FoundationContract;

/// Domain separator every namespace digest is taken over.
pub const NAMESPACE_DIGEST_DOMAIN: &[u8] = b"slingshot.runtime-namespace/1";

/// Characters of one name that reach the readable part of a namespace key.
pub const READABLE_NAME_CHARACTERS: usize = 24;

/// Character an unusable one becomes in the readable part of a key.
pub const READABLE_REPLACEMENT: char = '_';

/// Directory below the state root that holds one target's database.
pub const DATABASE_FILE_NAME: &str = "operations.sqlite3";

/// Directory below a target's state that holds its artifacts.
pub const ARTIFACT_DIRECTORY: &str = "artifacts";

/// Directory below a target's state that holds its maintenance records.
pub const MAINTENANCE_DIRECTORY: &str = "maintenance";

/// Directory below a target's state that holds its diagnostics.
pub const DIAGNOSTIC_DIRECTORY: &str = "diagnostics";

/// Directory below the state root that holds every target's state.
pub const TARGETS_DIRECTORY: &str = "targets";

/// File at the state root that holds the global installation record.
pub const INSTALLATION_RECORD_FILE_NAME: &str = "installation.json";

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
    /// A root is not one this process may use.
    #[error("{root} is not a directory this user alone owns")]
    RootNotPrivate {
        /// Which root.
        root: String,
    },
    /// The filesystem refused.
    #[error("the filesystem refused: {0}")]
    FilesystemRefused(String),
}

/// Returns the readable part one name contributes to a namespace key.
///
/// Everything a path component cannot safely hold becomes one replacement
/// character, and what is left is truncated. Two names can therefore escape to
/// the same text, which is exactly why the digest is appended to it rather than
/// replaced by it.
#[must_use]
pub fn readable_component(name: &str) -> String {
    name.chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '.' {
                character.to_ascii_lowercase()
            } else {
                READABLE_REPLACEMENT
            }
        })
        .take(READABLE_NAME_CHARACTERS)
        .collect()
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

impl RuntimeNamespace {
    /// Returns the readable key this namespace's directories are named with.
    ///
    /// The escaped names first, so a listing is legible, then the digest, which
    /// is what actually tells two namespaces apart.
    #[must_use]
    pub fn key(&self) -> String {
        format!(
            "{}-{}-{}",
            readable_component(&self.profile),
            readable_component(&self.environment),
            self.digest
        )
    }

    /// Returns the endpoint this namespace is reachable at.
    ///
    /// Resolved here and created elsewhere: naming an endpoint is a property of
    /// the namespace, and binding one is a property of the platform.
    ///
    /// # Errors
    ///
    /// Returns [`NamespaceFailure::TooLong`] when the address is beyond the
    /// bound the foundation contract declares for this platform.
    pub fn endpoint(
        &self,
        contract: &FoundationContract,
    ) -> Result<crate::platform_runtime::endpoint::EndpointAddress, NamespaceFailure> {
        crate::platform_runtime::endpoint::endpoint_address(
            contract,
            &self.runtime_root,
            &self.digest,
        )
        .map_err(|failure| match failure {
            crate::platform_runtime::failure::PlatformFailure::EndpointNameTooLong {
                length,
                limit,
            } => NamespaceFailure::TooLong { name: "endpoint address", length, limit },
            other => NamespaceFailure::FilesystemRefused(other.to_string()),
        })
    }

    /// Returns where this namespace's readiness record lives.
    #[must_use]
    pub fn readiness_path(&self) -> PathBuf {
        crate::platform_runtime::readiness::record_path(&self.runtime_root, &self.digest)
    }

    /// Returns where one target's durable state lives beneath `state_root`.
    ///
    /// The state root is supplied separately from the runtime root, and neither
    /// is derived from the other. A caller that had only one of them could not
    /// express the thing this separation exists for: a login that ended without
    /// the work ending with it.
    #[must_use]
    pub fn beneath(&self, state_root: &Path) -> PersistentTargetPaths {
        PersistentTargetPaths {
            state_root: state_root.to_path_buf(),
            target: state_root.join(TARGETS_DIRECTORY).join(self.key()),
        }
    }

    /// Creates this namespace's runtime directory, reachable by its owner alone.
    ///
    /// # Errors
    ///
    /// Returns [`NamespaceFailure::RootNotPrivate`] when a directory already
    /// there is not one this user alone owns, or
    /// [`NamespaceFailure::FilesystemRefused`].
    pub fn create_runtime_directory(&self) -> Result<(), NamespaceFailure> {
        create_private_directory(&self.runtime_root)
    }
}

/// Where one target's durable state lives, beneath the persistent root.
///
/// Every path here is a fixed name below a directory named by the namespace
/// key, so nothing a caller typed ever becomes a path component on its own. A
/// name carrying a separator, a parent marker, or an absolute prefix has
/// already been refused when the namespace was named, and what reaches the
/// filesystem is the escaped readable text plus a digest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistentTargetPaths {
    /// The root every target's state lives beneath.
    state_root: PathBuf,
    /// This target's own directory.
    target: PathBuf,
}

impl PersistentTargetPaths {
    /// Returns the root every target's state lives beneath.
    #[must_use]
    pub fn state_root(&self) -> &Path {
        &self.state_root
    }

    /// Returns this target's own directory.
    #[must_use]
    pub fn target_root(&self) -> &Path {
        &self.target
    }

    /// Returns where this target's operation database lives.
    #[must_use]
    pub fn database_path(&self) -> PathBuf {
        self.target.join(DATABASE_FILE_NAME)
    }

    /// Returns where this target's artifacts live.
    #[must_use]
    pub fn artifact_root(&self) -> PathBuf {
        self.target.join(ARTIFACT_DIRECTORY)
    }

    /// Returns where this target's maintenance records live.
    #[must_use]
    pub fn maintenance_root(&self) -> PathBuf {
        self.target.join(MAINTENANCE_DIRECTORY)
    }

    /// Returns where this target's diagnostics live.
    #[must_use]
    pub fn diagnostic_root(&self) -> PathBuf {
        self.target.join(DIAGNOSTIC_DIRECTORY)
    }

    /// Returns where the installation record every target shares lives.
    ///
    /// At the state root rather than inside a target, because one installation
    /// identity covers every target this user has.
    #[must_use]
    pub fn installation_record_path(&self) -> PathBuf {
        self.state_root.join(INSTALLATION_RECORD_FILE_NAME)
    }

    /// Creates every directory this target's state needs.
    ///
    /// # Errors
    ///
    /// Returns [`NamespaceFailure::RootNotPrivate`] when a directory already
    /// there is not one this user alone owns, or
    /// [`NamespaceFailure::FilesystemRefused`].
    pub fn create(&self) -> Result<(), NamespaceFailure> {
        create_private_directory(&self.state_root)?;
        create_private_directory(&self.state_root.join(TARGETS_DIRECTORY))?;
        create_private_directory(&self.target)?;
        create_private_directory(&self.artifact_root())?;
        create_private_directory(&self.maintenance_root())?;
        create_private_directory(&self.diagnostic_root())
    }
}

/// Creates one directory reachable by its owner alone, or validates the one there.
fn create_private_directory(path: &Path) -> Result<(), NamespaceFailure> {
    crate::platform_runtime::current_user::create_owner_only_directory(path)
        .map_err(|failure| NamespaceFailure::FilesystemRefused(failure.to_string()))?;
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|failure| NamespaceFailure::FilesystemRefused(failure.to_string()))?;
    let private = crate::platform_runtime::current_user::is_owner_only(path)
        .map_err(|failure| NamespaceFailure::FilesystemRefused(failure.to_string()))?;
    if !metadata.is_dir() || !private {
        return Err(NamespaceFailure::RootNotPrivate { root: path.display().to_string() });
    }
    Ok(())
}
