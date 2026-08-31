//! Streaming one verified artifact to a client, in bounded pieces.
//!
//! A transfer is three kinds of response in one order: a start that says what
//! is coming, some chunks, and an end that says whether what arrived was what
//! was promised. The end is the only place success is reported, and it is
//! reported only after the whole second pass agrees with the recorded digest
//! and length and the handle is still the file it was.
//!
//! That ordering is what makes the transfer trustworthy rather than merely
//! plausible. A client that received chunks and then a failure knows to discard
//! them; a client that received an end knows every byte it got was verified on
//! the way past. There is no arrangement of responses in which a client is told
//! a transfer succeeded and is wrong.
//!
//! A resumed transfer proves the part it is skipping. The prefix is read and
//! hashed through the same handle rather than sought over, because those are
//! exactly the bytes the client is asking to be trusted about - a seek would
//! leave the second-pass digest unable to say anything about them.
//!
//! Chunks are bounded by the contract, and a client asking for more gets the
//! bound rather than a refusal: it asked how much it would like, not how much
//! it insists on.

use slingshot_domain::daemon_runtime_contract::DaemonRuntimeContract;
use slingshot_storage::artifact_store::{
    ArtifactFailure, ArtifactMetadata, ArtifactStore, VerifiedArtifactReader,
};

/// What a client asked to read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferRequest {
    /// What it expects the content to be, checked before anything is emitted.
    pub expected_content_digest: String,
    /// How much of the artifact it already has.
    pub starting_offset: u64,
    /// How much it would like at a time.
    pub preferred_chunk_bytes: u64,
}

/// The first response of a transfer, sent before any bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferStart {
    /// The artifact being read.
    pub artifact_identifier: String,
    /// The digest every byte is checked against.
    pub content_digest: String,
    /// How many bytes the whole artifact holds.
    pub total_byte_length: u64,
    /// How many bytes this transfer sends, from the offset onwards.
    pub transferred_byte_length: u64,
}

/// Why a transfer could not start, or could not be trusted.
#[derive(Debug, thiserror::Error)]
pub enum TransferFailure {
    /// The caller expects content this artifact does not hold.
    #[error("the caller expects {expected}, and this artifact holds {holding}")]
    DigestMismatch {
        /// What the artifact holds.
        holding: String,
        /// What the caller expects.
        expected: String,
    },
    /// The caller resumed past the end of the artifact.
    #[error("this artifact holds {holding} bytes, and the caller resumed at {offset}")]
    OffsetPastEnd {
        /// How long the artifact is.
        holding: u64,
        /// Where the caller resumed.
        offset: u64,
    },
    /// The store refused.
    #[error(transparent)]
    Artifact(#[from] ArtifactFailure),
}

/// Returns the largest chunk one response may carry.
#[must_use]
pub fn maximum_chunk_bytes() -> u64 {
    DaemonRuntimeContract::embedded().limit("maximum_local_artifact_chunk_bytes")
}

/// Returns how large this transfer's chunks are.
///
/// The smaller of what the client would like and what the contract allows. A
/// client asking for more is given the bound rather than refused, because it
/// said what it would prefer rather than what it requires.
#[must_use]
pub fn chunk_bytes(preferred: u64) -> u64 {
    preferred.clamp(1, maximum_chunk_bytes())
}

/// One transfer in progress.
#[derive(Debug)]
pub struct ArtifactTransfer {
    /// How large this transfer's chunks are.
    chunk_bytes: usize,
    /// The verified handle every byte comes through.
    reader: VerifiedArtifactReader,
    /// How many bytes are still to send.
    remaining: u64,
}

impl ArtifactTransfer {
    /// Opens one transfer, verifying before it promises anything.
    ///
    /// The whole artifact is read and checked before the start response exists,
    /// so a client that receives a start has already been told the truth about
    /// what is coming.
    ///
    /// # Errors
    ///
    /// Returns [`TransferFailure::DigestMismatch`] when the caller expects
    /// other content, [`TransferFailure::OffsetPastEnd`] for a resume past the
    /// end, or a store failure.
    pub fn open(
        store: &ArtifactStore,
        metadata: &ArtifactMetadata,
        request: &TransferRequest,
    ) -> Result<(Self, TransferStart), TransferFailure> {
        if request.expected_content_digest != metadata.content_digest {
            return Err(TransferFailure::DigestMismatch {
                holding: metadata.content_digest.clone(),
                expected: request.expected_content_digest.clone(),
            });
        }
        if request.starting_offset > metadata.byte_length {
            return Err(TransferFailure::OffsetPastEnd {
                holding: metadata.byte_length,
                offset: request.starting_offset,
            });
        }
        let mut reader = store.open_verified(metadata)?;
        reader.discard_prefix(request.starting_offset)?;
        let remaining = metadata.byte_length - request.starting_offset;
        let start = TransferStart {
            artifact_identifier: metadata.artifact_identifier.as_text().to_owned(),
            content_digest: metadata.content_digest.clone(),
            total_byte_length: metadata.byte_length,
            transferred_byte_length: remaining,
        };
        let chunk_bytes =
            usize::try_from(chunk_bytes(request.preferred_chunk_bytes)).unwrap_or(usize::MAX);
        Ok((Self { chunk_bytes, reader, remaining }, start))
    }

    /// Returns the next chunk, or nothing when the suffix is done.
    ///
    /// Every chunk is the full length except the last, which is whatever
    /// remains. A client can therefore tell it has reached the end without
    /// being told, though it is told anyway.
    ///
    /// # Errors
    ///
    /// Returns [`TransferFailure`] when the store refuses.
    pub fn next_chunk(&mut self) -> Result<Option<Vec<u8>>, TransferFailure> {
        if self.remaining == 0 {
            return Ok(None);
        }
        let wanted = usize::try_from(self.remaining).unwrap_or(self.chunk_bytes);
        let mut buffer = vec![0_u8; wanted.min(self.chunk_bytes)];
        let read = self.reader.read_into(&mut buffer)?;
        if read == 0 {
            // The file ended before the recorded length did, which means it was
            // truncated while this transfer was reading it. Stopping here is
            // what lets the end report it: looping for bytes that will never
            // arrive would hang instead of failing.
            self.remaining = 0;
            return Ok(None);
        }
        buffer.truncate(read);
        self.remaining = self.remaining.saturating_sub(read as u64);
        Ok(Some(buffer))
    }

    /// Reports whether what was streamed was the artifact it claimed to be.
    ///
    /// The only place a transfer reports success. Everything before it is a
    /// promise; this is the check.
    ///
    /// # Errors
    ///
    /// Returns [`TransferFailure`] when the second pass disagrees with the
    /// recorded digest or length, or the handle is no longer the file it was.
    pub fn finish(self) -> Result<(), TransferFailure> {
        Ok(self.reader.finish()?)
    }
}
