//! What happens when a caller stops waiting.
//!
//! Stopping watching is not stopping work. The operation keeps its identifier
//! and the caller can come back to it, so nothing here cancels anything at the
//! daemon or the agent - an interrupt that cancelled would make a keystroke
//! destroy remote work, which is not what pressing it means.
//!
//! # What may be said depends entirely on how far it got
//!
//! The four phases are the four honest answers. Before the receipt nothing is
//! known about any operation; after it the operation exists and is named;
//! during a transfer the operation and the artifact are known; during a
//! maintenance fetch the target and the result are. A fifth answer that hedged
//! between them would leave a person unsure which of the four they are in,
//! which is the only thing they need.
//!
//! # A committed publication wins over a later signal
//!
//! The rename that makes a destination appear is the success. A signal arriving
//! at or after it cannot turn a finished thing into an interrupted one, so the
//! renderer runs to completion and the exit is zero.

use crate::exit_classification::{INTERRUPTED, SUCCESS};
use crate::machine_outcome_envelope::Interruption;

/// How far one invocation had got when the signal arrived.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Phase {
    /// The request was sent and no receipt has been validated.
    BeforeReceipt {
        /// The identifier a caller quotes to find out what happened.
        retry_operation_identifier: String,
    },
    /// The receipt was validated and the operation is being watched.
    Observing {
        /// Which operation.
        operation_identifier: String,
        /// Which revision it was admitted at.
        revision: u64,
    },
    /// An artifact is being fetched and nothing is published.
    FetchingArtifact {
        /// Which artifact.
        artifact_identifier: String,
        /// Which operation produced it.
        operation_identifier: String,
    },
    /// A maintenance result is being fetched and nothing is published.
    FetchingMaintenanceResult {
        /// Which partition.
        author_target_identity_digest: String,
        /// Which result.
        maintenance_result_identifier: String,
    },
    /// A publication or a final rendering has committed.
    Committed,
}

/// What one signal produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignalOutcome {
    /// The work that was committed stands, and the exit is success.
    CommittedWork,
    /// Nothing was committed, and this is the honest account of how far it got.
    Interrupted {
        /// What is said about it.
        interruption: Interruption,
    },
}

impl SignalOutcome {
    /// Returns the exit this outcome produces.
    #[must_use]
    pub fn exit(&self) -> i32 {
        match self {
            Self::CommittedWork => SUCCESS,
            Self::Interrupted { .. } => INTERRUPTED,
        }
    }

    /// Returns whether anything remote was asked to stop.
    ///
    /// Never. A keystroke that destroyed remote work is not what pressing one
    /// means, and the operation a caller walked away from is one they can come
    /// back to.
    #[must_use]
    pub fn asked_anything_to_stop(&self) -> bool {
        false
    }
}

/// Returns what a signal arriving in `phase` produces.
///
/// A committed publication wins. The rename that makes a destination appear is
/// the success, and a signal at or after it cannot turn a finished thing into
/// an interrupted one.
#[must_use]
pub fn on_signal(phase: &Phase) -> SignalOutcome {
    let interruption = match phase {
        Phase::Committed => return SignalOutcome::CommittedWork,
        Phase::BeforeReceipt { retry_operation_identifier } => {
            Interruption::PreReceipt { retry_identifier: retry_operation_identifier.clone() }
        }
        Phase::Observing { operation_identifier, revision } => Interruption::PostReceipt {
            operation_identifier: operation_identifier.clone(),
            revision: *revision,
        },
        Phase::FetchingArtifact { artifact_identifier, operation_identifier } => {
            Interruption::ArtifactTransfer {
                artifact_identifier: artifact_identifier.clone(),
                operation_identifier: operation_identifier.clone(),
            }
        }
        Phase::FetchingMaintenanceResult {
            author_target_identity_digest,
            maintenance_result_identifier,
        } => Interruption::MaintenanceResultTransfer {
            author_target_identity_digest: author_target_identity_digest.clone(),
            maintenance_result_identifier: maintenance_result_identifier.clone(),
        },
    };
    SignalOutcome::Interrupted { interruption }
}
