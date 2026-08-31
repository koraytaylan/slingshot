//! What running one operation looks like, without saying how it is run.
//!
//! The executor boundary exists so the daemon can be tested without a remote
//! system and shipped without a fake. Everything on this side of it is typed
//! and transport-free: a command, the target partition it belongs to, ports for
//! reporting progress and installing artifacts, and one closed outcome. There
//! are no frames here, no database handles, and no daemon state, because
//! anything that leaked through would be a thing a test double had to imitate
//! and could imitate wrongly.
//!
//! The outcome is closed and its variants are not interchangeable. Success
//! means the work happened and its results are available. A terminal failure
//! means the work ended, and it carries the pairing of kind and disposition the
//! domain validates rather than a free-text reason. Recovery-required means the
//! work has not ended: something is outstanding, the fact says what and how
//! sure the daemon is, and nobody may read it as either an ending or a success.
//!
//! Progress is best-effort by construction. A consumer that stopped listening
//! must never be able to stall or fail an execution, because the whole point of
//! reporting progress is that somebody might be watching - not that somebody
//! must be.

use crate::operation::{RecoveryFact, TerminalFailure};

/// Which operation is being run, and on whose behalf.
///
/// The target digest is here rather than the identity it digests, so an
/// executor can record and partition its work without ever holding the opaque
/// value that identity is made of.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ExecutionIdentity {
    /// How many times this operation has been attempted, including this one.
    pub attempt: u32,
    /// The partition this operation belongs to.
    pub author_target_identity_digest: String,
    /// The identifier its caller chose.
    pub operation_identifier: String,
}

/// One artifact an execution produced.
///
/// Metadata only. The bytes reached the store through the store's own
/// interface, and what crosses this boundary is the verified description of
/// where they ended up - never a path, and never the bytes again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProducedArtifact {
    /// The deterministic artifact identifier.
    pub artifact_identifier: String,
    /// The command-declared slot it fills.
    pub artifact_slot: String,
    /// Exactly how many bytes it holds.
    pub byte_length: u64,
    /// The digest of those bytes.
    pub content_digest: String,
    /// The bounded media type.
    pub media_type: String,
}

/// How an execution ended, or failed to end.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperationExecutorOutcome {
    /// The work happened and its results are available.
    Succeeded {
        /// Artifacts it produced, in slot order.
        artifacts: Vec<ProducedArtifact>,
        /// The canonical result, when it is small enough to travel inline.
        inline_result: Option<String>,
    },
    /// The work ended without succeeding.
    TerminalFailure {
        /// Why it ended, as a pairing the domain validates.
        failure: TerminalFailure,
    },
    /// The work has not ended, and this says what is outstanding.
    RecoveryRequired {
        /// What is outstanding, and how sure the daemon is about it.
        recovery: RecoveryFact,
    },
}

impl OperationExecutorOutcome {
    /// Returns whether this outcome ends the operation.
    ///
    /// Recovery-required is the one that does not, and reading it as an ending
    /// is the mistake this exists to make hard: an operation waiting on a
    /// retrieval that has not happened yet is neither finished nor failed.
    #[must_use]
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Succeeded { .. } | Self::TerminalFailure { .. })
    }

    /// Returns whether this outcome may be published as a result.
    #[must_use]
    pub fn publishes_a_result(&self) -> bool {
        matches!(self, Self::Succeeded { .. })
    }
}

/// Where an execution reports progress, when anyone is listening.
///
/// Every method returns nothing and cannot fail. A port that could refuse would
/// be a port an execution had to handle refusals from, and the first thing
/// anyone would write is code that treats a refusal as fatal - which is exactly
/// the coupling this avoids.
pub trait ProgressPort {
    /// Reports one bounded description of what is happening.
    fn report(&self, detail: &str);
}

/// One executor, which is one way of running a typed command.
pub trait OperationExecutor {
    /// Runs `command` for `identity`, reporting progress as it goes.
    ///
    /// # Errors
    ///
    /// None. Everything that can go wrong is an outcome rather than an error,
    /// because "the remote refused" and "the daemon could not reach it" are
    /// facts about the operation that have to be recorded, not exceptions that
    /// may be logged and dropped.
    fn execute(
        &self,
        identity: &ExecutionIdentity,
        command: &crate::command::catalog::Command,
        progress: &dyn ProgressPort,
    ) -> OperationExecutorOutcome;
}
