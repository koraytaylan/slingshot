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

/// One artifact arriving, verified and staged as it goes.
///
/// The destination appears only when this says it may: the bytes accumulate in
/// a staging file beside it, each one is bounded and hashed as it arrives, and
/// publication happens once the length and the digest both agree. Nothing
/// before that moment is visible to anybody else.
#[derive(Debug)]
pub struct Arrival {
    /// Where the bytes accumulate, beside the destination.
    staging_path: std::path::PathBuf,
    /// The file they accumulate in.
    staging: std::fs::File,
    /// Whether the daemon has said what is coming.
    transfer: Option<Transfer>,
    /// What the daemon declared, when it declared anything.
    declared: Option<DeclaredArtifact>,
    /// What the bytes have digested to so far.
    hasher: sha2::Sha256,
}

/// What one artifact transfer declared before its bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredArtifact {
    /// Which artifact.
    pub artifact_identifier: String,
    /// How long it is.
    pub byte_length: u64,
    /// What it digests to.
    pub content_digest: String,
    /// What kind of bytes it holds.
    pub media_type: String,
}

impl Arrival {
    /// Begins one arrival, staging at `staging`.
    ///
    /// # Errors
    ///
    /// Returns [`DownloadRefusal::DestinationUnusable`] when the staging file
    /// cannot be created.
    pub fn new(staging: std::path::PathBuf) -> Result<Self, DownloadRefusal> {
        use sha2::Digest as _;
        let file =
            std::fs::File::create(&staging).map_err(|_| DownloadRefusal::DestinationUnusable)?;
        Ok(Self {
            staging_path: staging,
            staging: file,
            transfer: None,
            declared: None,
            hasher: sha2::Sha256::new(),
        })
    }

    /// Takes one event from the daemon.
    ///
    /// # Errors
    ///
    /// Returns what is wrong with it as words a caller can act on: a chunk
    /// before the daemon said what was coming, more bytes than it declared, or
    /// a staging file that cannot be written.
    pub fn take(&mut self, event: crate::daemon_connection::ArtifactEvent) -> Result<(), String> {
        use sha2::Digest as _;
        match event {
            crate::daemon_connection::ArtifactEvent::Start {
                artifact_identifier,
                byte_length,
                content_digest,
                media_type,
            } => {
                if self.transfer.is_some() {
                    return Err("the daemon declared the artifact twice".to_owned());
                }
                self.transfer = Some(Transfer::of(byte_length, &content_digest));
                self.declared = Some(DeclaredArtifact {
                    artifact_identifier,
                    byte_length,
                    content_digest,
                    media_type,
                });
                Ok(())
            }
            crate::daemon_connection::ArtifactEvent::Chunk(bytes) => {
                let held = self
                    .transfer
                    .as_mut()
                    .ok_or_else(|| "the daemon sent bytes before saying what they are".to_owned())?;
                held.absorb(bytes.len() as u64).map_err(|refusal| refusal.to_string())?;
                self.hasher.update(&bytes);
                std::io::Write::write_all(&mut self.staging, &bytes)
                    .map_err(|failure| format!("the staged bytes could not be written: {failure}"))
            }
        }
    }

    /// Returns what the daemon declared, when it declared anything.
    #[must_use]
    pub fn declared(&self) -> Option<&DeclaredArtifact> {
        self.declared.as_ref()
    }

    /// Publishes the staged bytes at `destination`, or says why it may not.
    ///
    /// # Errors
    ///
    /// Returns [`DownloadRefusal`] when the transfer is short, when the bytes
    /// are not what the daemon described, or when the publication is refused.
    pub fn publish(&mut self, destination: &std::path::Path) -> Result<(), DownloadRefusal> {
        use sha2::Digest as _;
        let transfer = self.transfer.take().ok_or(DownloadRefusal::DestinationUnusable)?;
        let digest: String = self
            .hasher
            .clone()
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        if let Err(refusal) = transfer.require_publishable(&digest) {
            self.discard();
            return Err(refusal);
        }
        std::io::Write::flush(&mut self.staging).map_err(|_| DownloadRefusal::DestinationUnusable)?;
        if let Err(refusal) = publish(&self.staging_path, destination) {
            self.discard();
            return Err(refusal);
        }
        Ok(())
    }

    /// Removes what a refused transfer staged.
    ///
    /// A partial file left beside the caller's name is worse than no file: the
    /// next command that reads it has no way to tell it from a whole one, and
    /// the fact that it is a resumable remnant is this build's private
    /// knowledge rather than the caller's.
    pub fn discard(&mut self) {
        std::fs::remove_file(&self.staging_path).ok();
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
    publish_no_replace(staging, destination)
}

/// Publishes through the operating system's atomic no-replace primitive.
///
/// A prior existence check is intentionally absent: checking and then naming
/// is the race this adapter exists to remove. The destination is resolved
/// against a retained directory handle, while the staged bytes are published
/// from their already-open authenticated handle after synchronization.
#[cfg(target_os = "linux")]
fn publish_no_replace(
    staging: &std::path::Path,
    destination: &std::path::Path,
) -> Result<(), DownloadRefusal> {
    use rustix::fs::{AtFlags, CWD, Mode, OFlags, linkat, openat};
    use std::os::fd::AsRawFd as _;
    use std::os::unix::fs::MetadataExt as _;

    let directory = destination.parent().ok_or(DownloadRefusal::DestinationUnusable)?;
    if staging.parent() != Some(directory) {
        return Err(DownloadRefusal::DestinationUnusable);
    }
    let staging_name = staging.file_name().ok_or(DownloadRefusal::DestinationUnusable)?;
    let destination_name = destination.file_name().ok_or(DownloadRefusal::DestinationUnusable)?;
    let held = std::fs::File::open(directory).map_err(|_| DownloadRefusal::DestinationUnusable)?;
    let staged = openat(
        &held,
        staging_name,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|_| DownloadRefusal::DestinationUnusable)?;
    let staged = std::fs::File::from(staged);
    let identity = staged.metadata().map_err(|_| DownloadRefusal::DestinationUnusable)?;
    if !identity.is_file() || identity.nlink() != 1 {
        return Err(DownloadRefusal::DestinationUnusable);
    }
    staged.sync_all().map_err(|_| DownloadRefusal::DestinationUnusable)?;
    let retained_handle = format!("/proc/self/fd/{}", staged.as_raw_fd());
    linkat(CWD, retained_handle, &held, destination_name, AtFlags::SYMLINK_FOLLOW).map_err(
        |failure| {
            if failure == rustix::io::Errno::EXIST {
                DownloadRefusal::DestinationOccupied
            } else {
                DownloadRefusal::DestinationUnusable
            }
        },
    )?;
    let staged_name_still_names_the_verified_file =
        std::fs::symlink_metadata(staging).is_ok_and(|current| {
            current.is_file() && current.dev() == identity.dev() && current.ino() == identity.ino()
        });
    if staged_name_still_names_the_verified_file {
        std::fs::remove_file(staging).map_err(|_| DownloadRefusal::DestinationUnusable)?;
    }
    held.sync_all().map_err(|_| DownloadRefusal::DestinationUnusable)
}

/// Publishes with the portable standard-library no-replace primitive.
///
/// `hard_link` is atomic with respect to the destination name and never
/// replaces an existing directory entry. The staging path is checked without
/// following links before it is linked, and the staged file is synchronized
/// before publication. This path intentionally uses only safe standard-library
/// APIs so production code remains free of `unsafe` blocks on platforms where
/// the Linux descriptor-based adapter is unavailable.
#[cfg(not(target_os = "linux"))]
fn publish_no_replace(
    staging: &std::path::Path,
    destination: &std::path::Path,
) -> Result<(), DownloadRefusal> {
    let directory = destination.parent().ok_or(DownloadRefusal::DestinationUnusable)?;
    if staging.parent() != Some(directory) {
        return Err(DownloadRefusal::DestinationUnusable);
    }
    let staged =
        std::fs::symlink_metadata(staging).map_err(|_| DownloadRefusal::DestinationUnusable)?;
    if !staged.is_file() {
        return Err(DownloadRefusal::DestinationUnusable);
    }
    std::fs::File::open(staging)
        .and_then(|file| file.sync_all())
        .map_err(|_| DownloadRefusal::DestinationUnusable)?;
    std::fs::hard_link(staging, destination).map_err(|failure| {
        if failure.kind() == std::io::ErrorKind::AlreadyExists {
            DownloadRefusal::DestinationOccupied
        } else {
            DownloadRefusal::DestinationUnusable
        }
    })?;
    std::fs::remove_file(staging).map_err(|_| DownloadRefusal::DestinationUnusable)
}
