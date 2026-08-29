//! Long-lived wrapper for an extracted secret.
//!
//! [`SecretValue`] is the only long-lived typed representation of an extracted
//! password, private key, client secret, signed assertion, or access token, and
//! [`SensitiveConfigurationDocument`] is the temporary buffer a source document
//! is read into before its secrets are extracted. Both render as
//! [`REDACTED_RENDERING`], reach their bytes only through a call whose name
//! says why the bytes escape, and carry no serialization, equality, ordering,
//! hashing, or fingerprinting interface, so no caller can obtain a stable
//! comparison oracle over a low-entropy secret.
//!
//! # Zeroization claim
//!
//! Both wrappers overwrite the heap buffer they own with zero bytes when they
//! are scrubbed, replaced, disposed of, or dropped, and a constructor that
//! must copy a caller-supplied vector overwrites that vector before releasing
//! it. The claim covers nothing else. Bytes an operating-system call already
//! copied into a pipe, a page cache, or a swapped page; an immutable buffer a
//! parser or another dependency allocated from lent bytes; memory an allocator
//! reuses or an optimizer keeps in a register or a spilled stack slot; and
//! bytes a caller still owns elsewhere are all outside it.

use secrecy::zeroize::Zeroize;
use secrecy::{ExposeSecret, ExposeSecretMut, SecretSlice};

/// Text every rendering of a secret-bearing wrapper produces.
///
/// The rendering is fixed: it never varies with the secret's length, content,
/// or presence, so a reader of a diagnostic learns nothing about the value.
pub const REDACTED_RENDERING: &str = "[redacted]";

/// An extracted secret.
///
/// The value owns its bytes and hands them out only through
/// [`SecretValue::expose_secret_bytes`]. It has no `Clone`, `PartialEq`,
/// `Hash`, `Ord`, `Serialize`, or digest interface, so it cannot be copied
/// implicitly, compared, ordered, keyed on, written to a document, or reduced
/// to a fingerprint that would survive redaction.
///
/// See the [module zeroization claim](self#zeroization-claim) for exactly which
/// memory the overwriting covers.
pub struct SecretValue {
    /// The owned secret bytes.
    bytes: SecretSlice<u8>,
}

impl SecretValue {
    /// Takes ownership of `bytes` as a secret.
    ///
    /// When `bytes` holds spare capacity the secret is copied into an exactly
    /// sized buffer and `bytes` is overwritten with zeroes before it is
    /// released, so the surplus allocation does not outlive the call holding a
    /// readable copy.
    #[must_use]
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self { bytes: SecretSlice::new(own_exactly(bytes)) }
    }

    /// Takes ownership of `text` as a secret, preserving its exact bytes.
    ///
    /// The text receives no Unicode normalization, so a password that differs
    /// only in composition remains a different secret.
    #[must_use]
    pub fn from_text(text: String) -> Self {
        Self::from_bytes(text.into_bytes())
    }

    /// Returns the secret bytes.
    ///
    /// Every call site is a place the secret leaves the wrapper, so the name
    /// says so rather than reading as an ordinary accessor.
    #[must_use]
    pub fn expose_secret_bytes(&self) -> &[u8] {
        self.bytes.expose_secret()
    }

    /// Returns how many bytes the secret occupies.
    ///
    /// The length is not secret: it is already implied by the bound each
    /// document field is validated against.
    #[must_use]
    pub fn secret_byte_length(&self) -> usize {
        self.bytes.expose_secret().len()
    }

    /// Overwrites the owned buffer with zero bytes and returns how many bytes
    /// it overwrote.
    ///
    /// The buffer keeps its length, so a caller can observe that the wrapper
    /// zeroed the memory it owns rather than merely dropping a reference to it.
    pub fn scrub(&mut self) -> usize {
        let exposed = self.bytes.expose_secret_mut();
        let scrubbed = exposed.len();
        exposed.zeroize();
        scrubbed
    }

    /// Replaces the secret with `replacement`, returning how many bytes of the
    /// previous secret were overwritten.
    ///
    /// The previous buffer is zeroed before it is released, so a rotation never
    /// leaves the superseded secret behind in a freed allocation.
    pub fn replace(&mut self, replacement: Vec<u8>) -> usize {
        let scrubbed = self.scrub();
        self.bytes = SecretSlice::new(own_exactly(replacement));
        scrubbed
    }
}

impl ::core::fmt::Display for SecretValue {
    fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        formatter.write_str(REDACTED_RENDERING)
    }
}

impl ::core::fmt::Debug for SecretValue {
    fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        formatter.write_str(REDACTED_RENDERING)
    }
}

/// A configuration source document held in memory while it is inspected.
///
/// The buffer exists only between reading a source file and extracting the
/// nonsecret values and [`SecretValue`]s it carries, because a profile or
/// credential document can contain a secret anywhere in its bytes. It lends its
/// bytes to a digest, an inspection, or a parser through a call named for that
/// purpose and returns no byte slice of its own, so an accidental
/// `document.bytes()` cannot exist.
///
/// See the [module zeroization claim](self#zeroization-claim) for exactly which
/// memory the overwriting covers; in particular a parser that keeps an owned
/// copy of the lent bytes is outside it.
pub struct SensitiveConfigurationDocument {
    /// The owned document bytes.
    bytes: SecretSlice<u8>,
}

impl SensitiveConfigurationDocument {
    /// Takes ownership of one source document's exact bytes.
    ///
    /// When `bytes` holds spare capacity the document is copied into an exactly
    /// sized buffer and `bytes` is overwritten with zeroes before it is
    /// released.
    #[must_use]
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self { bytes: SecretSlice::new(own_exactly(bytes)) }
    }

    /// Returns how many bytes the document occupies.
    ///
    /// The length is not secret: it is the value every document-size bound is
    /// checked against.
    #[must_use]
    pub fn document_byte_length(&self) -> usize {
        self.bytes.expose_secret().len()
    }

    /// Lends the document bytes to a digest computation and returns its result.
    ///
    /// The digest of a source document is secret-adjacent, because a document
    /// can carry a low-entropy secret. The caller decides where the digest may
    /// go; this wrapper only makes the bytes reachable for the computation.
    pub fn lend_bytes_for_digest<Outcome>(&self, digest: impl FnOnce(&[u8]) -> Outcome) -> Outcome {
        digest(self.bytes.expose_secret())
    }

    /// Lends the document bytes to a structural inspection and returns its
    /// result.
    ///
    /// An inspection answers a question about the document's shape, such as
    /// which references it names, without carrying document bytes out.
    pub fn lend_bytes_for_inspection<Outcome>(
        &self,
        inspect: impl FnOnce(&[u8]) -> Outcome,
    ) -> Outcome {
        inspect(self.bytes.expose_secret())
    }

    /// Decodes the document as text and lends it to a parser.
    ///
    /// # Errors
    ///
    /// Returns [`SensitiveDocumentNotUnicode`] when the bytes are not valid
    /// Unicode text. The failure names no byte offset, because a position
    /// inside a secret-bearing document is itself an oracle; the caller
    /// reports the source class and stage it already knows instead.
    pub fn lend_text_for_parsing<Outcome>(
        &self,
        parse: impl FnOnce(&str) -> Outcome,
    ) -> Result<Outcome, SensitiveDocumentNotUnicode> {
        let text = core::str::from_utf8(self.bytes.expose_secret())
            .map_err(|_| SensitiveDocumentNotUnicode)?;
        Ok(parse(text))
    }

    /// Overwrites the owned buffer with zero bytes and returns how many bytes
    /// it overwrote.
    ///
    /// Disposal consumes the document, so no caller can lend its bytes again
    /// after the buffer has been cleared.
    pub fn dispose(mut self) -> usize {
        let exposed = self.bytes.expose_secret_mut();
        let scrubbed = exposed.len();
        exposed.zeroize();
        scrubbed
    }
}

impl ::core::fmt::Display for SensitiveConfigurationDocument {
    fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        formatter.write_str(REDACTED_RENDERING)
    }
}

impl ::core::fmt::Debug for SensitiveConfigurationDocument {
    fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        formatter.write_str(REDACTED_RENDERING)
    }
}

/// A sensitive document could not be decoded as Unicode text.
///
/// The failure carries no position, excerpt, length, or cause, because every
/// one of those is derived from bytes that may be secret.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the configuration document is not valid unicode text")]
pub struct SensitiveDocumentNotUnicode;

/// Returns `bytes` in a buffer whose capacity equals its length.
///
/// `Vec::into_boxed_slice` reallocates when the vector holds spare capacity,
/// which would leave the secret readable in the released allocation. Copying
/// first and zeroing the original keeps the surplus from outliving this call.
fn own_exactly(mut bytes: Vec<u8>) -> Box<[u8]> {
    if bytes.capacity() == bytes.len() {
        return bytes.into_boxed_slice();
    }
    let mut owned = Vec::with_capacity(bytes.len());
    owned.extend_from_slice(&bytes);
    bytes.zeroize();
    owned.into_boxed_slice()
}
