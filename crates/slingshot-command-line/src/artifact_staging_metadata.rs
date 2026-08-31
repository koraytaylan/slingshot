//! Remembering what a partial download was, so a retry can finish it.
//!
//! A transfer that was interrupted leaves bytes on disk that are worth keeping
//! and worthless without their facts. Without knowing what they are of, how
//! long they should be, and what they should digest to, a retry has two
//! choices: start again, or publish what it happens to have. The first wastes
//! the transfer and the second is worse than wasting it.
//!
//! # Two identities, and never a default between them
//!
//! What is being fetched is either an operation's artifact or a maintenance
//! result, and the two shapes are disjoint. A maintenance result has no
//! operation, so a record that defaulted a missing operation identifier would
//! invent an operation the daemon never created - and a later read would then
//! look for it.
//!
//! # The state is closed and it is ordered
//!
//! Transferring, ready to publish, published. A retry may only resume from a
//! state it recognizes, against facts that agree with what is on disk, so a
//! sidecar from another transfer is a collision rather than a head start.

use serde::{Deserialize, Serialize};

/// What a staged transfer is of.
///
/// Two shapes, disjoint by construction. Nothing defaults one from the other,
/// because a maintenance result genuinely has no operation and inventing one
/// would send a later read looking for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "payload", rename_all = "snake_case", deny_unknown_fields)]
pub enum StagedPayload {
    /// One artifact of one operation.
    OperationArtifact {
        /// Which artifact.
        artifact_identifier: String,
        /// Which operation produced it.
        operation_identifier: String,
    },
    /// One maintenance result, which belongs to no operation.
    MaintenanceResult {
        /// Which result.
        maintenance_result_identifier: String,
    },
}

/// How far one staged transfer has got.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferState {
    /// Bytes are still arriving.
    Transferring,
    /// Every byte arrived and verified, and nothing is published yet.
    ReadyToPublish,
    /// The destination exists and holds exactly these bytes.
    Published,
}

/// What a staged transfer records beside its bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StagingRecord {
    /// Which partition it belongs to.
    pub author_target_identity_digest: String,
    /// What it digests to when it is whole.
    pub content_digest: String,
    /// What is being fetched.
    pub payload: StagedPayload,
    /// Which environment revision it was fetched under.
    pub selected_environment_revision: String,
    /// How far it has got.
    pub state: TransferState,
    /// How long it is when it is whole.
    pub total_length: u64,
    /// How much has arrived and verified so far.
    pub verified_length: u64,
}

/// Why a staged transfer cannot be resumed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ResumeRefusal {
    /// The record is about something else.
    #[error("the staged transfer beside this destination is of something else")]
    AnotherPayload,
    /// The record was made in another partition or under another revision.
    #[error("the staged transfer beside this destination belongs to another target")]
    AnotherTarget,
    /// The record and the bytes on disk disagree about how much arrived.
    #[error("the staged bytes are {actual} long and the record says {recorded}")]
    LengthDisagrees {
        /// How long the file is.
        actual: u64,
        /// How long the record says it is.
        recorded: u64,
    },
    /// The record says more arrived than the whole thing is.
    #[error("a staged transfer cannot have verified more than the whole")]
    VerifiedBeyondTotal,
}

impl StagingRecord {
    /// Requires this record to describe the transfer that is being retried.
    ///
    /// Everything, and against the bytes actually on disk. A record that
    /// matched on identity and disagreed on length would let a retry resume
    /// from a position the file never reached.
    ///
    /// # Errors
    ///
    /// Returns [`ResumeRefusal`] naming the first thing that disagrees.
    pub fn require_resumable(
        &self,
        payload: &StagedPayload,
        author_target_identity_digest: &str,
        selected_environment_revision: &str,
        staged_length: u64,
    ) -> Result<TransferState, ResumeRefusal> {
        if &self.payload != payload {
            return Err(ResumeRefusal::AnotherPayload);
        }
        if self.author_target_identity_digest != author_target_identity_digest
            || self.selected_environment_revision != selected_environment_revision
        {
            return Err(ResumeRefusal::AnotherTarget);
        }
        if self.verified_length > self.total_length {
            return Err(ResumeRefusal::VerifiedBeyondTotal);
        }
        if self.verified_length != staged_length {
            return Err(ResumeRefusal::LengthDisagrees {
                actual: staged_length,
                recorded: self.verified_length,
            });
        }
        Ok(self.state)
    }

    /// Returns whether this transfer has everything it needs to publish.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.verified_length == self.total_length
    }
}
