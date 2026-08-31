//! What this repository is compatible with, and how that is established.
//!
//! One manifest, read rather than assumed, naming the exact external commit,
//! the protocol revision, the handler format, and the two upstream contracts
//! this integration is bound to. A compatibility claim that could fall back to
//! a default is a claim about nothing, so every field is required and no value
//! has an alternative spelling.
//!
//! # An origin is parsed by a closed grammar, never normalized into one
//!
//! Three spellings name the same repository - the canonical address, the secure
//! shell form, and the short form - and every other spelling is refused rather
//! than repaired. Repairing one is how a lookalike authority becomes acceptable:
//! a host with user information in front of it, a port after it, or an escape
//! inside it reads as the pinned repository to a lenient parser and as somewhere
//! else entirely to whatever does the fetching.
//!
//! # A seed is bounded in every dimension it has
//!
//! A supplied Cargo home is an arbitrary directory somebody hands this build. A
//! limit on some of its dimensions is a limit on none of them, so the count of
//! files, the count of directories, the length of a component, the length of a
//! path, the depth, the size of one file, and the size of all of them together
//! are each bounded, and the first violation in one fixed traversal order is
//! the diagnostic.

use serde::Deserialize;

/// Where the manifest lives.
pub const MANIFEST_PATH: &str = "compatibility/finite-state-machine.toml";

/// The format the manifest declares.
pub const MANIFEST_FORMAT: &str = "slingshot.finite-state-machine-compatibility/1";

/// How many characters a commit is named by.
pub const COMMIT_CHARACTERS: usize = 40;

/// The host the pinned repository lives on.
pub const PINNED_HOST: &str = "github.com";

/// The scheme a canonical origin is written in.
pub const CANONICAL_SCHEME: &str = "https://";

/// The user a secure-shell origin names.
pub const SECURE_SHELL_USER: &str = "git@";

/// The suffix an origin may end with, and which names nothing extra.
pub const OPTIONAL_SUFFIX: &str = ".git";

/// The manifest, exactly as it is committed.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FiniteStateMachineCompatibilityPin {
    /// The digest of the author-agent transport contract this is bound to.
    pub author_agent_transport_contract_sha256: String,
    /// The format of that contract.
    pub author_agent_transport_contract_format: String,
    /// What a supplied Cargo home may be.
    pub cargo_home_seed: SeedLimits,
    /// The exact commit this integration is compatible with.
    pub commit: String,
    /// The digest of the daemon runtime contract this is bound to.
    pub daemon_runtime_contract_sha256: String,
    /// The format of that contract.
    pub daemon_runtime_contract_format: String,
    /// The format this manifest declares.
    pub format: String,
    /// The format the external handler table is written in.
    pub handler_format: String,
    /// The protocol revision the external executor negotiates.
    pub model_context_protocol_revision: String,
    /// The repository the commit lives in.
    pub repository: String,
    /// How a workflow names one registry command effect.
    pub workflow_effect_operation_key: OperationKeyContract,
}

/// What a supplied Cargo home may be, in every dimension it has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeedLimits {
    /// How many bytes every file together may hold.
    pub maximum_aggregate_file_bytes: u64,
    /// How many bytes one path component may hold.
    pub maximum_component_utf8_bytes: u64,
    /// How deep the tree may go, with the root at zero.
    pub maximum_depth: u64,
    /// How many directories it may hold, including the root.
    pub maximum_directories: u64,
    /// How many bytes one file may hold.
    pub maximum_file_bytes: u64,
    /// How many files it may hold.
    pub maximum_files: u64,
    /// How many bytes one relative path may hold.
    pub maximum_relative_path_utf8_bytes: u64,
}

/// How a workflow names one registry command effect.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationKeyContract {
    /// What every key begins with.
    pub key_prefix: String,
    /// How many bytes one key may hold.
    pub maximum_key_bytes: u64,
    /// How many bytes one input may hold.
    pub maximum_input_utf8_bytes: u64,
    /// How many bytes one suffix may hold.
    pub maximum_suffix_bytes: u64,
    /// The format the preimage declares.
    pub preimage_format: String,
    /// Every suffix a key may carry, and no others.
    pub suffixes: Vec<String>,
}

/// The exact tuple this integration is compatible with.
///
/// Ordered, and compared whole. Comparing the parts one at a time somewhere
/// else is how two of them end up compared and the third forgotten.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FiniteStateMachineCompatibilityIdentity {
    /// Which transport contract, and its digest.
    pub author_agent_transport_contract: (String, String),
    /// Which commit.
    pub commit: String,
    /// Which runtime contract, and its digest.
    pub daemon_runtime_contract: (String, String),
    /// Which handler format.
    pub handler_format: String,
    /// Which protocol revision.
    pub model_context_protocol_revision: String,
    /// Which repository.
    pub repository: String,
}

/// Why the compatibility pin is refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PinRefusal {
    /// The manifest is not readable as this format.
    #[error("the compatibility manifest could not be read: {0}")]
    Unreadable(String),
    /// The manifest declares another format.
    #[error("the compatibility manifest declares {0} rather than {MANIFEST_FORMAT}")]
    ForeignFormat(String),
    /// A commit is not forty hexadecimal characters.
    #[error("a commit is {COMMIT_CHARACTERS} hexadecimal characters, and this is not")]
    CommitUnusable,
    /// A recorded digest is not what the bytes beside it produce.
    #[error("{0} does not match the bytes it is recorded against")]
    DigestDrifted(String),
    /// The origin is not one this grammar admits.
    #[error("{0} is not a spelling of the pinned repository this build accepts")]
    OriginUnusable(String),
}

impl FiniteStateMachineCompatibilityPin {
    /// Returns the pin one manifest text declares.
    ///
    /// # Errors
    ///
    /// Returns [`PinRefusal`] naming the first rule the manifest breaks.
    pub fn parse(text: &str) -> Result<Self, PinRefusal> {
        let held: Self =
            toml::from_str(text).map_err(|failure| PinRefusal::Unreadable(failure.to_string()))?;
        if held.format != MANIFEST_FORMAT {
            return Err(PinRefusal::ForeignFormat(held.format.clone()));
        }
        require_commit(&held.commit)?;
        Ok(held)
    }

    /// Returns the tuple this pin makes.
    #[must_use]
    pub fn identity(&self) -> FiniteStateMachineCompatibilityIdentity {
        FiniteStateMachineCompatibilityIdentity {
            author_agent_transport_contract: (
                self.author_agent_transport_contract_format.clone(),
                self.author_agent_transport_contract_sha256.clone(),
            ),
            commit: self.commit.clone(),
            daemon_runtime_contract: (
                self.daemon_runtime_contract_format.clone(),
                self.daemon_runtime_contract_sha256.clone(),
            ),
            handler_format: self.handler_format.clone(),
            model_context_protocol_revision: self.model_context_protocol_revision.clone(),
            repository: self.repository.clone(),
        }
    }

    /// Requires both recorded contract digests to be what their bytes produce.
    ///
    /// Recomputed rather than compared against another recorded string, because
    /// two recorded strings agreeing proves only that somebody wrote the same
    /// thing twice.
    ///
    /// # Errors
    ///
    /// Returns [`PinRefusal::DigestDrifted`] naming the contract that moved.
    pub fn require_contract_digests(
        &self,
        daemon_runtime_bytes: &[u8],
        author_transport_bytes: &[u8],
    ) -> Result<(), PinRefusal> {
        let compared = [
            (
                "the daemon runtime contract",
                daemon_runtime_bytes,
                &self.daemon_runtime_contract_sha256,
            ),
            (
                "the author-agent transport contract",
                author_transport_bytes,
                &self.author_agent_transport_contract_sha256,
            ),
        ];
        for (named, bytes, recorded) in compared {
            if digest_of(bytes) != *recorded {
                return Err(PinRefusal::DigestDrifted(named.to_owned()));
            }
        }
        Ok(())
    }
}

/// Returns the lowercase hexadecimal digest of some bytes.
fn digest_of(bytes: &[u8]) -> String {
    use sha2::Digest;
    let mut digest = sha2::Sha256::new();
    digest.update(bytes);
    digest.finalize().iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Requires one commit to be named the way a commit is named.
fn require_commit(commit: &str) -> Result<(), PinRefusal> {
    let shaped = commit.len() == COMMIT_CHARACTERS
        && commit.chars().all(|held| held.is_ascii_digit() || ('a'..='f').contains(&held));
    if shaped { Ok(()) } else { Err(PinRefusal::CommitUnusable) }
}

/// Returns the canonical address one origin spelling names.
///
/// Three spellings are admitted and everything else is refused. What is
/// returned is the canonical form, so a comparison against the pin is a
/// comparison of bytes rather than of intentions.
///
/// # Errors
///
/// Returns [`PinRefusal::OriginUnusable`] for every other spelling, including
/// the ones a lenient parser would repair.
pub fn canonical_origin(spelling: &str) -> Result<String, PinRefusal> {
    let refused = || PinRefusal::OriginUnusable(spelling.to_owned());
    let rest = if let Some(held) = spelling.strip_prefix("https://") {
        held.strip_prefix(&format!("{PINNED_HOST}/")).ok_or_else(refused)?
    } else if let Some(held) = spelling.strip_prefix("ssh://git@") {
        held.strip_prefix(&format!("{PINNED_HOST}/")).ok_or_else(refused)?
    } else if let Some(held) = spelling.strip_prefix(SECURE_SHELL_USER) {
        held.strip_prefix(&format!("{PINNED_HOST}:")).ok_or_else(refused)?
    } else {
        return Err(refused());
    };
    let named = rest.strip_suffix(OPTIONAL_SUFFIX).unwrap_or(rest);
    let mut segments = named.split('/');
    let owner = segments.next().unwrap_or_default();
    let repository = segments.next().unwrap_or_default();
    if segments.next().is_some() {
        return Err(refused());
    }
    require_segment(owner)?;
    require_segment(repository)?;
    Ok(format!("{CANONICAL_SCHEME}{PINNED_HOST}/{owner}/{repository}"))
}

/// Requires one path segment to be a name this grammar admits.
fn require_segment(segment: &str) -> Result<(), PinRefusal> {
    let refused = || PinRefusal::OriginUnusable(segment.to_owned());
    if segment.is_empty() || segment == "." || segment == ".." {
        return Err(refused());
    }
    let admitted = segment
        .chars()
        .all(|held| held.is_ascii_alphanumeric() || held == '-' || held == '_' || held == '.');
    if admitted { Ok(()) } else { Err(refused()) }
}
