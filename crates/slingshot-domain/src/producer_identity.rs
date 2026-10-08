//! Stable cooperative queue identity, independent of authentication and operation identity.

use sha2::Digest as _;

use crate::daemon_runtime_contract::{DIGEST_CHARACTERS, DaemonRuntimeContract};

/// Separates producer labels from every other SHA-256 input vocabulary.
const DOMAIN: &[u8] = b"slingshot.producer-identity/1\0";

/// An invalid label or opaque rendering; no caller-controlled bytes are disclosed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error(
    "SLINGSHOT_PRODUCER requires a nonempty label within the runtime byte bound, without control characters; its wire identity must be a canonical SHA-256 digest"
)]
pub struct InvalidProducerIdentity;

/// A queue label's domain-separated digest, scoped by the retained target partition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProducerIdentity(String);

impl ProducerIdentity {
    /// Hashes the exact UTF-8 bytes, without case folding or whitespace normalization.
    ///
    /// # Errors
    ///
    /// Rejects empty labels, Unicode control characters, and labels above the byte bound.
    pub fn from_label(label: &str) -> Result<Self, InvalidProducerIdentity> {
        if label.is_empty()
            || label.len() as u64
                > DaemonRuntimeContract::embedded().limit("maximum_producer_label_bytes")
            || label.chars().any(char::is_control)
        {
            return Err(InvalidProducerIdentity);
        }
        let mut hash = sha2::Sha256::new();
        hash.update(DOMAIN);
        hash.update(label.as_bytes());
        Ok(Self(hash.finalize().iter().map(|octet| format!("{octet:02x}")).collect()))
    }

    /// Reads the opaque identity supplied on the local protocol.
    ///
    /// # Errors
    ///
    /// Rejects anything except exactly sixty-four lowercase hexadecimal characters.
    pub fn parse(identity: &str) -> Result<Self, InvalidProducerIdentity> {
        if identity.len() != DIGEST_CHARACTERS
            || !identity
                .bytes()
                .all(|octet| octet.is_ascii_digit() || (b'a'..=b'f').contains(&octet))
        {
            return Err(InvalidProducerIdentity);
        }
        Ok(Self(identity.to_owned()))
    }

    /// Returns only the digest; the raw label is never retained here.
    #[must_use]
    pub fn as_text(&self) -> &str {
        &self.0
    }
}
