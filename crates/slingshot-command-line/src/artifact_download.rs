//! Fetching an operation's artifact to a place the caller chose.
//!
//! The daemon owns whether the artifact exists; this owns where it lands. The
//! whole design is about one moment: the rename that makes the destination
//! appear. Everything before it is private and disposable, everything after it
//! is visible and final, and nothing in between is observable.
//!
//! # Publication is the success, and it happens once
//!
//! Bytes accumulate in a staging file beside the destination - beside it, so
//! the publication is a rename rather than a copy, because a rename across
//! filesystems is not atomic and the atomicity is the point. The rename happens
//! only after the length and the digest both agree, and it does not overwrite:
//! a destination that already exists is a collision, not a target.
//!
//! # An interrupt before it costs nothing and an interrupt after it costs nothing
//!
//! Before the rename there is no new destination and the private state is
//! resumable. After it the destination is whole, and a rerun that finds a
//! matching published receipt re-renders the original success rather than
//! fetching again or publishing twice.

use crate::artifact_staging_metadata::{StagedPayload, StagingRecord, TransferState};

/// Why one fetch could not be completed or published.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DownloadRefusal {
    /// More bytes arrived than the artifact holds.
    #[error("this artifact holds {expected} bytes, and {actual} arrived")]
    LengthDrifted {
        /// How long it actually is.
        actual: u64,
        /// How long it should be.
        expected: u64,
    },
    /// The bytes are not the ones the daemon described.
    #[error("this artifact does not digest to what the daemon said it would")]
    DigestDrifted,
    /// Something is already at the destination.
    #[error("something already exists at that destination, and nothing here overwrites")]
    DestinationOccupied,
    /// The destination or a staging file is not an ordinary file.
    #[error("a destination is an ordinary file this user owns, and this is not")]
    DestinationUnusable,
}

/// One transfer in progress, verified as it goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transfer {
    /// What the whole artifact digests to.
    content_digest: String,
    /// How much has arrived.
    received: u64,
    /// How long the whole artifact is.
    total_length: u64,
}

impl Transfer {
    /// Returns a transfer of `total_length` bytes digesting to `content_digest`.
    #[must_use]
    pub fn of(total_length: u64, content_digest: &str) -> Self {
        Self { content_digest: content_digest.to_owned(), received: 0, total_length }
    }

    /// Returns a transfer resumed from what a record says already arrived.
    #[must_use]
    pub fn resumed(record: &StagingRecord) -> Self {
        Self {
            content_digest: record.content_digest.clone(),
            received: record.verified_length,
            total_length: record.total_length,
        }
    }

    /// Returns how much has arrived.
    #[must_use]
    pub fn received(&self) -> u64 {
        self.received
    }

    /// Records `bytes` more, refusing before they are written anywhere.
    ///
    /// # Errors
    ///
    /// Returns [`DownloadRefusal::LengthDrifted`] when more arrives than the
    /// artifact holds, which is checked as the bytes come rather than at the
    /// end: a daemon that sent more has already cost the disk it was written to.
    pub fn absorb(&mut self, bytes: u64) -> Result<(), DownloadRefusal> {
        let reached = self.received.saturating_add(bytes);
        if reached > self.total_length {
            return Err(DownloadRefusal::LengthDrifted {
                actual: reached,
                expected: self.total_length,
            });
        }
        self.received = reached;
        Ok(())
    }

    /// Returns whether every byte has arrived.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.received == self.total_length
    }

    /// Requires this transfer to be one that may be published.
    ///
    /// # Errors
    ///
    /// Returns [`DownloadRefusal::LengthDrifted`] or
    /// [`DownloadRefusal::DigestDrifted`].
    pub fn require_publishable(&self, observed_digest: &str) -> Result<(), DownloadRefusal> {
        if !self.is_complete() {
            return Err(DownloadRefusal::LengthDrifted {
                actual: self.received,
                expected: self.total_length,
            });
        }
        if observed_digest != self.content_digest {
            return Err(DownloadRefusal::DigestDrifted);
        }
        Ok(())
    }

    /// Returns the record this transfer would write beside its bytes.
    #[must_use]
    pub fn record(
        &self,
        payload: StagedPayload,
        author_target_identity_digest: &str,
        selected_environment_revision: &str,
        state: TransferState,
    ) -> StagingRecord {
        StagingRecord {
            author_target_identity_digest: author_target_identity_digest.to_owned(),
            content_digest: self.content_digest.clone(),
            payload,
            selected_environment_revision: selected_environment_revision.to_owned(),
            state,
            total_length: self.total_length,
            verified_length: self.received,
        }
    }
}

/// What a rerun found already done.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PriorWork {
    /// Nothing, so the transfer starts.
    None,
    /// Bytes arrived and stopped, so the transfer resumes.
    Resumable,
    /// Everything arrived and verified, so only the publication remains.
    ReadyToPublish,
    /// It was published, so the original success is re-rendered.
    AlreadyPublished,
}

/// Returns what a rerun may do, given what it found beside the destination.
///
/// A published receipt is believed only when the destination matches every
/// recorded fact. A missing or mismatched one is an ordinary collision, and the
/// destination is preserved rather than replaced - because the alternative is
/// overwriting a file this command did not create.
#[must_use]
pub fn prior_work(record: Option<&StagingRecord>, destination_matches: bool) -> PriorWork {
    match record.map(|held| held.state) {
        None => PriorWork::None,
        Some(TransferState::Transferring) => PriorWork::Resumable,
        Some(TransferState::ReadyToPublish) => PriorWork::ReadyToPublish,
        Some(TransferState::Published) if destination_matches => PriorWork::AlreadyPublished,
        Some(TransferState::Published) => PriorWork::None,
    }
}

/// Publishes the staged bytes at `destination`, without overwriting.
///
/// This call is the success. Everything before it is private and disposable;
/// after it the destination is whole. Nothing here removes an existing file: a
/// destination that is already there is a collision, and treating it as a
/// target would destroy something this command did not create.
///
/// # Errors
///
/// Returns [`DownloadRefusal::DestinationOccupied`] or
/// [`DownloadRefusal::DestinationUnusable`].
pub fn publish(
    staging: &std::path::Path,
    destination: &std::path::Path,
) -> Result<(), DownloadRefusal> {
    if destination.symlink_metadata().is_ok() {
        return Err(DownloadRefusal::DestinationOccupied);
    }
    std::fs::rename(staging, destination).map_err(|_| DownloadRefusal::DestinationUnusable)
}
