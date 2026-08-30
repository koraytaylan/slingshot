//! The proof that a repeated identifier describes the same work.
//!
//! A caller chooses its operation identifier before it contacts the daemon, so
//! the same identifier can arrive twice. Whether that is a retry of one request
//! or two different requests wearing one name is exactly the question this
//! answers: two requests share a fingerprint only when they describe the same
//! target, the same environment revision, and the same command.
//!
//! # What goes in, and what deliberately does not
//!
//! The target enters as Plan 0002's opaque digest, used directly. Nothing here
//! parses a deployment, an address, a principal, or a contract member out of
//! it, and nothing hashes its rendering instead of its bytes. Those would be
//! two ways of computing one thing, and the version that was wrong would be
//! whichever was written second.
//!
//! Everything a request carries that is not the work stays out: the request
//! identifier, the wait cursor, timestamps, display preferences, connection
//! state, publisher metadata, and every credential. A fingerprint that moved
//! when a timestamp did would make every retry a conflict.
//!
//! # What a stable fingerprint means, and what it does not
//!
//! Genuine same-principal credential rotation leaves the upstream target and
//! revision alone, so the fingerprint holds and a retry is still a retry. A
//! changed metascope, a changed trust-policy identity, or an explicit-restart
//! policy change arrives as a changed *revision*, and a changed principal
//! arrives as a changed *target*; either changes the fingerprint. This module
//! never decides which of those happened - it is handed two opaque values and
//! it hashes them.
//!
//! The revision is also stored separately and compared on its own. A
//! fingerprint match is evidence that two requests agree; it is not a substitute
//! for checking the provenance the daemon started from.

use serde::{Deserialize, Serialize};
use sha2::Digest as _;

/// Version of the encoding a fingerprint hashes.
///
/// Part of the preimage, so a future encoding cannot produce a value that
/// collides with one this version produced.
pub const FINGERPRINT_ENCODING_VERSION: &str = "slingshot.command-fingerprint/1";

/// Byte separating two parts of the preimage.
///
/// Cannot occur in any part, so no pair of different inputs produces one
/// preimage by running two fields together.
pub const FIELD_SEPARATOR: u8 = 0;

/// Octets a SHA-256 digest occupies.
const DIGEST_OCTETS: usize = 32;

/// Characters a digest is rendered with.
pub const DIGEST_CHARACTERS: usize = DIGEST_OCTETS * 2;

/// Reason a fingerprint could not be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum FingerprintFailure {
    /// The rendering is not sixty-four lowercase hexadecimal characters.
    #[error(
        "a command fingerprint is exactly {DIGEST_CHARACTERS} lowercase hexadecimal characters"
    )]
    NotCanonical,
    /// A part of the preimage carries the byte that separates parts.
    #[error("no part of a fingerprint preimage carries its separator")]
    SeparatorInPart,
}

/// What one request describes, as the fingerprint sees it.
///
/// Four values and nothing else. Each is supplied whole by whoever owns it, and
/// this type is deliberately unable to look inside any of them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FingerprintInput {
    /// Plan 0002's opaque target digest, used as it stands.
    pub author_target_identity_digest: String,
    /// The command's typed content, canonically written.
    pub canonical_command: String,
    /// Which command this is.
    pub command_wire_name: String,
    /// The command contract version it was written against.
    pub command_semantic_contract_version: String,
    /// Plan 0002's exact environment revision.
    pub selected_environment_revision: String,
}

/// The proof two requests describe the same work.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct CommandFingerprint {
    /// The digest, in lowercase hexadecimal.
    value: String,
}

impl CommandFingerprint {
    /// Returns the fingerprint `input` describes.
    ///
    /// # Errors
    ///
    /// Returns [`FingerprintFailure::SeparatorInPart`] when any part carries
    /// the separator, which would let two different inputs produce one
    /// preimage.
    pub fn derive(input: &FingerprintInput) -> Result<Self, FingerprintFailure> {
        let parts = [
            FINGERPRINT_ENCODING_VERSION,
            &input.author_target_identity_digest,
            &input.selected_environment_revision,
            &input.command_semantic_contract_version,
            &input.command_wire_name,
            &input.canonical_command,
        ];
        if parts.iter().any(|part| part.as_bytes().contains(&FIELD_SEPARATOR)) {
            return Err(FingerprintFailure::SeparatorInPart);
        }
        let mut hasher = sha2::Sha256::new();
        for (position, part) in parts.iter().enumerate() {
            if position > 0 {
                hasher.update([FIELD_SEPARATOR]);
            }
            hasher.update(part.as_bytes());
        }
        Ok(Self { value: render(&hasher.finalize()) })
    }

    /// Returns the fingerprint `spelling` names.
    ///
    /// # Errors
    ///
    /// Returns [`FingerprintFailure::NotCanonical`] for anything but exactly
    /// sixty-four lowercase hexadecimal characters.
    pub fn parse(spelling: &str) -> Result<Self, FingerprintFailure> {
        let canonical = spelling.len() == DIGEST_CHARACTERS
            && spelling
                .chars()
                .all(|character| character.is_ascii_hexdigit() && !character.is_ascii_uppercase());
        if canonical {
            Ok(Self { value: spelling.to_owned() })
        } else {
            Err(FingerprintFailure::NotCanonical)
        }
    }

    /// Returns the fingerprint, in lowercase hexadecimal.
    #[must_use]
    pub fn as_text(&self) -> &str {
        &self.value
    }
}

impl TryFrom<String> for CommandFingerprint {
    type Error = FingerprintFailure;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<CommandFingerprint> for String {
    fn from(fingerprint: CommandFingerprint) -> Self {
        fingerprint.value
    }
}

/// Returns `octets` in lowercase hexadecimal.
fn render(octets: &[u8]) -> String {
    octets.iter().map(|octet| format!("{octet:02x}")).collect()
}

/// Whether a repeated identifier is a retry or a collision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepeatedIdentifier {
    /// The same work, described the same way.
    Retry,
    /// A different request wearing the same name.
    Conflict,
}

/// Returns what a repeated identifier is.
///
/// Both the fingerprint and the revision have to match. The revision is
/// compared separately rather than trusted through the fingerprint, because it
/// is startup-snapshot provenance the daemon checked for itself and a digest
/// agreeing with a digest is not the same as a daemon agreeing with what it
/// started from.
#[must_use]
pub fn classify_repeat(
    stored_fingerprint: &CommandFingerprint,
    stored_revision: &str,
    offered_fingerprint: &CommandFingerprint,
    offered_revision: &str,
) -> RepeatedIdentifier {
    if stored_fingerprint == offered_fingerprint && stored_revision == offered_revision {
        RepeatedIdentifier::Retry
    } else {
        RepeatedIdentifier::Conflict
    }
}
