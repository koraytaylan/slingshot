//! What an operation is, as a durable fact rather than a memory of one.
//!
//! A daemon that inferred an operation's state from what its process happened
//! to remember would lose that state on restart and would answer differently
//! depending on which process a caller reached. So the lifecycle is a fold: a
//! sequence of recorded facts, each of which either advances it or is refused,
//! and each advance takes the revision with it.
//!
//! The revision is what makes concurrent facts safe. Two writers proposing the
//! same next state at the same revision are proposing the same thing; two
//! proposing different ones cannot both win.
//!
//! # The distinction the whole model turns on
//!
//! `OperationExecutionCertainty` never says a remote command succeeded. It says
//! how *unsure* the daemon is that it ran. Proven success is a different fact,
//! carried by a different shape with no certainty field at all, because a value
//! that could hold both would eventually be asked which it meant.
//!
//! That is why recovery evidence and terminal disposition are unions rather
//! than structs with optional fields. `RecoveryWindowExpired` in particular is
//! only for recovery that timed out *before* any remote outcome was known -
//! when the remote provably succeeded and only local work remains, the honest
//! terminal failure is `ResultUnavailable` with authoritative remote success,
//! and the two must not be interchangeable.
//!
//! # What is deliberately not here
//!
//! Connection health, process identity, waiter state, and transport errors.
//! None of them is a fact about the operation, and folding them in would make
//! the lifecycle change when nothing about the work did.

use serde::{Deserialize, Serialize};

/// Where one operation is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationLifecycleState {
    /// Admitted and waiting.
    Queued,
    /// Being handed to the remote.
    Submitting,
    /// The remote took it.
    Accepted,
    /// The remote is working on it.
    Running,
    /// It finished, and the result is known.
    Succeeded,
    /// It ended without succeeding.
    Failed,
}

impl OperationLifecycleState {
    /// Returns whether nothing can change this state.
    #[must_use]
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed)
    }

    /// Returns whether this state may become `next`.
    ///
    /// Forward only, one step at a time, and never out of a terminal state. A
    /// state that could be re-entered would make a duplicate fact
    /// indistinguishable from a regression.
    #[must_use]
    pub fn may_become(self, next: Self) -> bool {
        match self {
            Self::Queued => matches!(next, Self::Submitting | Self::Succeeded | Self::Failed),
            Self::Submitting => matches!(next, Self::Accepted | Self::Succeeded | Self::Failed),
            Self::Accepted => matches!(next, Self::Running | Self::Succeeded | Self::Failed),
            Self::Running => matches!(next, Self::Succeeded | Self::Failed),
            Self::Succeeded | Self::Failed => false,
        }
    }
}

/// How sure the daemon is that a remote command ran.
///
/// Never says one succeeded. That is a different fact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationExecutionCertainty {
    /// It provably did not run.
    ConfirmedNotExecuted,
    /// Whether it was submitted is unknown.
    SubmissionUnknown,
    /// It was submitted and its outcome is unknown.
    RemoteOutcomeUnknown,
}

impl OperationExecutionCertainty {
    /// Returns whether this certainty leaves the outcome unresolved.
    #[must_use]
    pub fn is_unresolved(self) -> bool {
        matches!(self, Self::SubmissionUnknown | Self::RemoteOutcomeUnknown)
    }
}

/// What is known about execution while recovery is outstanding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "evidence", rename_all = "snake_case")]
pub enum RecoveryExecutionEvidence {
    /// Execution is unproved, and this is how unproved.
    ExecutionCertainty {
        /// How sure the daemon is.
        certainty: OperationExecutionCertainty,
    },
    /// The remote succeeded and local work remains.
    AuthoritativeRemoteSuccess,
}

/// Why one operation needs recovery.
///
/// Closed, and each category fixes which evidence may accompany it. A category
/// about an unresolved submission cannot carry proven success, and one about
/// retrieving a proven result cannot carry uncertainty about whether the work
/// happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryCategory {
    /// A submission whose fate is unknown.
    AmbiguousSubmission,
    /// A reconnection to the remote's event stream.
    EventReconnection,
    /// A lookup of an operation the remote may still hold.
    OperationLookup,
    /// A recovery from the remote's job snapshot.
    JobSnapshotRecovery,
    /// Retrieving the result of a command that provably succeeded.
    ResultAcquisition,
    /// Transferring an artifact of a command that provably succeeded.
    ArtifactTransfer,
    /// Persistent storage that cannot currently hold the result.
    PersistentCapacityUnavailable,
}

impl RecoveryCategory {
    /// Returns whether this category may carry `evidence`.
    ///
    /// The four unresolved categories carry a certainty; the three that follow
    /// a proven success carry no certainty at all. Allowing either to carry the
    /// other's shape would let a retry replay work that already happened, or
    /// abandon work that did not.
    #[must_use]
    pub fn admits(self, evidence: RecoveryExecutionEvidence) -> bool {
        let proven = matches!(
            self,
            Self::ResultAcquisition | Self::ArtifactTransfer | Self::PersistentCapacityUnavailable
        );
        matches!(
            (proven, evidence),
            (false, RecoveryExecutionEvidence::ExecutionCertainty { .. })
                | (true, RecoveryExecutionEvidence::AuthoritativeRemoteSuccess)
        )
    }
}

/// Why an operation ended without succeeding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalFailureKind {
    /// The remote refused it.
    Rejected,
    /// The remote ran it and it failed.
    RemoteFailed,
    /// It succeeded and its result could not be obtained.
    ResultUnavailable,
    /// Recovery ran out of time before any outcome was known.
    RecoveryWindowExpired,
    /// The remote no longer knows about it.
    RemoteStateLost,
    /// Something did not verify.
    IntegrityFailure,
    /// Retrying stopped being allowed.
    RetryPolicyExhausted,
}

/// What a terminal failure asserts about effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "disposition", rename_all = "snake_case")]
pub enum TerminalFailureDisposition {
    /// It provably did not run.
    AuthoritativeNonExecution {
        /// Always [`OperationExecutionCertainty::ConfirmedNotExecuted`].
        certainty: OperationExecutionCertainty,
    },
    /// It ran and failed.
    AuthoritativeRemoteFailure,
    /// It ran and succeeded, and something after that did not.
    AuthoritativeRemoteSuccess,
    /// Nobody can tell, so the daemon fails closed.
    FailClosedIndeterminate {
        /// Which unknown this is.
        certainty: OperationExecutionCertainty,
    },
}

impl TerminalFailureDisposition {
    /// Returns whether this disposition carries a certainty it may.
    #[must_use]
    pub fn is_consistent(self) -> bool {
        match self {
            Self::AuthoritativeNonExecution { certainty } => {
                certainty == OperationExecutionCertainty::ConfirmedNotExecuted
            }
            Self::FailClosedIndeterminate { certainty } => certainty.is_unresolved(),
            Self::AuthoritativeRemoteFailure | Self::AuthoritativeRemoteSuccess => true,
        }
    }
}

/// Returns whether `kind` may be recorded with `disposition`.
///
/// The pairing is the contract, not a convention. `RecoveryWindowExpired` is
/// only for recovery that ran out before any outcome was known: once the remote
/// provably succeeded and only retrieval remains, the honest failure is
/// `ResultUnavailable` with authoritative remote success. Letting the two swap
/// would report proven work as never attempted.
#[must_use]
pub fn terminal_pairing_is_legal(
    kind: TerminalFailureKind,
    disposition: TerminalFailureDisposition,
) -> bool {
    if !disposition.is_consistent() {
        return false;
    }
    match kind {
        TerminalFailureKind::Rejected => {
            matches!(disposition, TerminalFailureDisposition::AuthoritativeNonExecution { .. })
        }
        TerminalFailureKind::RemoteFailed => {
            matches!(disposition, TerminalFailureDisposition::AuthoritativeRemoteFailure)
        }
        TerminalFailureKind::ResultUnavailable => {
            matches!(disposition, TerminalFailureDisposition::AuthoritativeRemoteSuccess)
        }
        TerminalFailureKind::RecoveryWindowExpired
        | TerminalFailureKind::RemoteStateLost
        | TerminalFailureKind::IntegrityFailure => {
            matches!(disposition, TerminalFailureDisposition::FailClosedIndeterminate { .. })
        }
        TerminalFailureKind::RetryPolicyExhausted => matches!(
            disposition,
            TerminalFailureDisposition::AuthoritativeNonExecution { .. }
                | TerminalFailureDisposition::FailClosedIndeterminate { .. }
        ),
    }
}

/// Reason a fact could not be folded into an operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum LifecycleFailure {
    /// The operation has already ended.
    #[error("a terminal operation takes no further fact")]
    AlreadyTerminal,
    /// The state cannot become the one proposed.
    #[error("an operation moves forward one state at a time")]
    TransitionNotAllowed,
    /// A fact repeats a state with different particulars.
    #[error("two facts propose the same state with different particulars")]
    ConflictingFact,
    /// A recovery category and its evidence do not belong together.
    #[error("a recovery category carries only the evidence its kind allows")]
    EvidenceNotAdmitted,
    /// A terminal kind and its disposition do not belong together.
    #[error("a terminal failure carries only the disposition its kind allows")]
    DispositionNotAdmitted,
}

/// One row of a listing page.
///
/// Enough to decide what to ask about next, and no payload. The terminal kind
/// travels because knowing an operation failed without knowing how is not
/// useful; the disposition does not, because a caller that needs it is asking
/// about one operation rather than scanning a page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationListing {
    /// Who asked, when a caller said.
    pub caller_identity: Option<String>,
    /// Where it sits in its partition's arrival order.
    pub enqueue_sequence: u64,
    /// The state it has reached.
    pub lifecycle_state: OperationLifecycleState,
    /// The identifier its caller chose.
    pub operation_identifier: String,
    /// The revision this row is at.
    pub revision: u64,
    /// When it settled, if it has.
    pub settled_at_unix_milliseconds: Option<u64>,
    /// Why it ended, when it ended without succeeding.
    pub terminal_failure_kind: Option<TerminalFailureKind>,
    /// The workflow it belongs to, when it belongs to one.
    pub workflow_correlation_identifier: Option<String>,
}

/// One durable proof that a recovery resume was already applied.
///
/// A domain value rather than a storage one, because what it records is a fact
/// about an operation: which revision a resume made eligible, against which
/// environment revision, for which source. Storage persists it; it does not
/// define it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveryResumeReceipt {
    /// The revision the resume committed.
    pub applied_operation_revision: u64,
    /// The operation it resumed.
    pub operation_identifier: String,
    /// When it was recorded.
    pub recorded_at_unix_milliseconds: u64,
    /// The environment revision it was recorded against.
    pub selected_environment_revision: String,
    /// The source it is keyed by.
    pub source_fingerprint: String,
}

/// Where one operation's result went.
///
/// Recorded separately from the lifecycle because it answers a different
/// question: reaching [`OperationLifecycleState::Succeeded`] says the work
/// happened, and this says where what it produced can be found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResultDisposition {
    /// Small enough to travel in the response itself.
    Inline,
    /// Kept as a content-addressed artifact.
    Artifact,
}

/// What one operation is, as of one revision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationRecord {
    /// The most recent progress note, when there is one.
    pub latest_progress: Option<String>,
    /// Where the operation is.
    pub lifecycle_state: OperationLifecycleState,
    /// Recovery outstanding against it, when there is any.
    pub outstanding_recovery: Option<RecoveryFact>,
    /// How many facts have been folded in.
    pub revision: u64,
    /// Why it ended, when it ended without succeeding.
    pub terminal_failure: Option<TerminalFailure>,
}

/// One recovery that is outstanding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveryFact {
    /// How many attempts have been made.
    pub attempt_count: u32,
    /// Which kind of recovery this is.
    pub category: RecoveryCategory,
    /// Bounded description, with nothing a command supplied in it.
    pub detail: String,
    /// What is known about execution.
    pub evidence: RecoveryExecutionEvidence,
    /// Whether a person may resume it by hand.
    pub manual_resume_eligible: bool,
    /// Milliseconds to wait before trying again.
    pub retry_delay_milliseconds: u64,
    /// When the delay was measured from.
    pub retry_observed_at_unix_milliseconds: u64,
}

/// One terminal failure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalFailure {
    /// What it asserts about effect.
    pub disposition: TerminalFailureDisposition,
    /// Why it ended.
    pub kind: TerminalFailureKind,
    /// Bounded description.
    pub metadata: Option<String>,
}

/// One fact offered to an operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "fact", rename_all = "snake_case")]
pub enum OperationFact {
    /// The operation reached a new state.
    Lifecycle {
        /// State it reached.
        lifecycle_state: OperationLifecycleState,
    },
    /// Something worth reporting happened.
    Progress {
        /// Bounded description.
        detail: String,
    },
    /// Recovery is outstanding.
    Recovery {
        /// What is outstanding.
        recovery: RecoveryFact,
    },
    /// The operation ended without succeeding.
    Terminal {
        /// Why it ended.
        failure: TerminalFailure,
    },
}

impl OperationRecord {
    /// Returns a freshly admitted operation.
    #[must_use]
    pub fn admitted() -> Self {
        Self {
            latest_progress: None,
            lifecycle_state: OperationLifecycleState::Queued,
            outstanding_recovery: None,
            revision: 1,
            terminal_failure: None,
        }
    }

    /// Folds one fact into this operation.
    ///
    /// A fact that changes nothing is a no-op at the same revision: two writers
    /// recording the same thing have recorded one thing. A fact that changes
    /// something takes the revision with it, once.
    ///
    /// # Errors
    ///
    /// Returns [`LifecycleFailure`] naming the first rule the fact breaks.
    pub fn fold(&self, fact: &OperationFact) -> Result<Self, LifecycleFailure> {
        match fact {
            OperationFact::Lifecycle { lifecycle_state } => self.fold_lifecycle(*lifecycle_state),
            OperationFact::Progress { detail } => Ok(self.fold_progress(detail)),
            OperationFact::Recovery { recovery } => self.fold_recovery(recovery),
            OperationFact::Terminal { failure } => self.fold_terminal(failure),
        }
    }

    /// Folds one new lifecycle state.
    fn fold_lifecycle(&self, next: OperationLifecycleState) -> Result<Self, LifecycleFailure> {
        if self.lifecycle_state == next {
            return Ok(self.clone());
        }
        if self.lifecycle_state.is_terminal() {
            return Err(LifecycleFailure::AlreadyTerminal);
        }
        if !self.lifecycle_state.may_become(next) {
            return Err(LifecycleFailure::TransitionNotAllowed);
        }
        Ok(Self { lifecycle_state: next, revision: self.revision + 1, ..self.clone() })
    }

    /// Folds one progress note.
    ///
    /// Progress does not reach a terminal operation, and it is dropped rather
    /// than refused: a late note about work that has finished is not an error,
    /// it is just no longer news.
    fn fold_progress(&self, detail: &str) -> Self {
        if self.lifecycle_state.is_terminal() || self.latest_progress.as_deref() == Some(detail) {
            return self.clone();
        }
        Self {
            latest_progress: Some(detail.to_owned()),
            revision: self.revision + 1,
            ..self.clone()
        }
    }

    /// Folds one recovery fact.
    fn fold_recovery(&self, recovery: &RecoveryFact) -> Result<Self, LifecycleFailure> {
        if self.lifecycle_state.is_terminal() {
            return Err(LifecycleFailure::AlreadyTerminal);
        }
        if !recovery.category.admits(recovery.evidence) {
            return Err(LifecycleFailure::EvidenceNotAdmitted);
        }
        if self.outstanding_recovery.as_ref() == Some(recovery) {
            return Ok(self.clone());
        }
        Ok(Self {
            outstanding_recovery: Some(recovery.clone()),
            revision: self.revision + 1,
            ..self.clone()
        })
    }

    /// Folds one terminal failure.
    fn fold_terminal(&self, failure: &TerminalFailure) -> Result<Self, LifecycleFailure> {
        if !terminal_pairing_is_legal(failure.kind, failure.disposition) {
            return Err(LifecycleFailure::DispositionNotAdmitted);
        }
        match (&self.terminal_failure, self.lifecycle_state.is_terminal()) {
            (Some(held), _) if held == failure => Ok(self.clone()),
            (Some(_), _) => Err(LifecycleFailure::ConflictingFact),
            (None, true) => Err(LifecycleFailure::ConflictingFact),
            (None, false) => Ok(Self {
                lifecycle_state: OperationLifecycleState::Failed,
                outstanding_recovery: None,
                revision: self.revision + 1,
                terminal_failure: Some(failure.clone()),
                ..self.clone()
            }),
        }
    }
}

/// Returns the monotonic deadline one recovery has after a restart.
///
/// Wall time between the observation and now is clamped to the original delay,
/// so a clock that moved backwards waits at most what it would have, and one
/// that moved forwards makes the retry eligible immediately. Idempotency never
/// depends on this being exact - it only decides when a retry becomes allowed,
/// and a retry is safe whenever it happens.
#[must_use]
pub fn remaining_delay_milliseconds(recovery: &RecoveryFact, now_unix_milliseconds: u64) -> u64 {
    let elapsed = now_unix_milliseconds
        .saturating_sub(recovery.retry_observed_at_unix_milliseconds)
        .min(recovery.retry_delay_milliseconds);
    recovery.retry_delay_milliseconds.saturating_sub(elapsed)
}
