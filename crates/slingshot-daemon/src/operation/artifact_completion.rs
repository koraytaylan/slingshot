//! Reserving before fetching, and publishing only what was proved.
//!
//! Capacity is reserved before the request is issued and before a staging file
//! exists, because a transfer that discovers there is no room has already spent
//! the disk and the bandwidth it was meant to protect. A refusal therefore
//! reads no body, creates no file, publishes nothing, and schedules nothing
//! automatic: it records that the work succeeded remotely and waits for a
//! person to make room.
//!
//! # A failed attempt keeps its names
//!
//! The partial file goes and the uncommitted reservation is released, but the
//! mapping from operation and slot to local artifact stays. So a retry - live
//! or after a restart - asks for the same artifact under the same identifiers
//! rather than allocating new ones, and a second, different body for an
//! identifier already verified is an integrity conflict that preserves the
//! original rather than replacing it.

use slingshot_agent_connection::artifact_download::{
    ArtifactTransfer, DownloadRefusal, ExpectedArtifact, TransferEnd,
};

/// Where one artifact lives locally, once it is mapped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactMapping {
    /// What the operation is called at the agent.
    pub agent_operation_identifier: String,
    /// Which partition it belongs to.
    pub author_target_identity_digest: String,
    /// What the local artifact is called.
    pub local_artifact_identifier: String,
    /// Which slot it fills.
    pub artifact_slot: String,
}

/// What one completion attempt produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompletionOutcome {
    /// The artifact is published, atomically, under its mapping.
    Published(Box<ArtifactMapping>),
    /// There is no room, and the remote success is recorded instead.
    PersistentCapacityUnavailable {
        /// How long the agent will still hold it.
        remaining_retention_milliseconds: u64,
    },
    /// The attempt failed and the mapping is kept for the next one.
    Retryable {
        /// What went wrong.
        refusal: Box<DownloadRefusal>,
    },
    /// A second, different body arrived for an identifier already verified.
    IntegrityConflict,
}

impl CompletionOutcome {
    /// Returns whether anything was written where a reader could see it.
    #[must_use]
    pub fn published_anything(&self) -> bool {
        matches!(self, Self::Published(_))
    }

    /// Returns whether this attempt may be made again without asking a person.
    #[must_use]
    pub fn permits_automatic_retry(&self) -> bool {
        matches!(self, Self::Retryable { .. })
    }
}

/// Where an artifact is reserved, staged, and published.
pub trait ArtifactSink: ::core::fmt::Debug {
    /// Reserves exactly `bytes`, before any request is issued.
    fn reserve(&self, bytes: u64) -> bool;

    /// Releases a reservation that was never committed.
    fn release(&self, bytes: u64);

    /// Removes whatever a failed attempt left behind.
    fn discard_partial(&self, mapping: &ArtifactMapping);

    /// Publishes the staged bytes atomically, under `mapping`.
    fn publish(&self, mapping: &ArtifactMapping, digest: &str);

    /// Returns what was already published under `mapping`, when anything was.
    fn published_digest(&self, mapping: &ArtifactMapping) -> Option<String>;
}

/// Returns what one completion attempt produces.
///
/// The order is the design: the reservation, then the transfer, then the proof,
/// then the publication. Nothing later can rescue a step that was skipped, and
/// nothing earlier is allowed to write where a reader could see it.
#[must_use]
pub fn complete(
    sink: &dyn ArtifactSink,
    mapping: &ArtifactMapping,
    expected: &ExpectedArtifact,
    transfer: &ArtifactTransfer,
    proof: (TransferEnd, &str, u64),
) -> CompletionOutcome {
    let (end, observed_digest, remaining_retention_milliseconds) = proof;
    if let Some(published) = sink.published_digest(mapping) {
        return if published == observed_digest {
            CompletionOutcome::Published(Box::new(mapping.clone()))
        } else {
            CompletionOutcome::IntegrityConflict
        };
    }
    if !sink.reserve(expected.byte_length) {
        return CompletionOutcome::PersistentCapacityUnavailable {
            remaining_retention_milliseconds,
        };
    }
    if let Err(refusal) = transfer.require_publishable(end, observed_digest) {
        sink.discard_partial(mapping);
        sink.release(expected.byte_length);
        return CompletionOutcome::Retryable { refusal: Box::new(refusal) };
    }
    sink.publish(mapping, observed_digest);
    CompletionOutcome::Published(Box::new(mapping.clone()))
}
