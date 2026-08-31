//! Answering what one operation is doing, and what it produced.
//!
//! Two questions with one rule between them: a client never infers. Status says
//! what the row holds, and result says what the operation produced or why it
//! did not, both as closed shapes a caller can match on rather than strings a
//! caller has to interpret. Anywhere the domain distinguishes two things - work
//! that has not ended from work that failed, a remote that provably succeeded
//! from one whose outcome is unknown - these answers distinguish them too, and
//! never fill in a value the row does not hold.
//!
//! Both are keyed by target digest and operation identifier together. A lookup
//! that could reach across partitions would let a client ask one target about
//! another target's work, which is a question nobody should be able to ask.
//!
//! Artifacts are verified before their bytes are reachable, and a missing or
//! corrupt one is a failure of the read rather than of the operation. What the
//! operation did is settled; the bytes not being retrievable now does not
//! unsettle it, and rewriting a terminal row because a file went missing would
//! destroy the record of work that actually happened.

use slingshot_domain::operation::{
    OperationLifecycleState, OperationListing, RecoveryFact, TerminalFailure,
    terminal_pairing_is_legal,
};
use slingshot_storage::operation_repository::{
    OperationRepository, OperationSummary, RepositoryFailure, ResultDisposition,
};

/// What one operation is doing, as its row holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationStatus {
    /// The partition it belongs to, which is not a secret.
    pub author_target_identity_digest: String,
    /// The bounded progress note, when it has one.
    pub latest_progress: Option<String>,
    /// The state it has reached.
    pub lifecycle_state: OperationLifecycleState,
    /// The identifier its caller chose.
    pub operation_identifier: String,
    /// What it is waiting on, when it is waiting on something.
    pub outstanding_recovery: Option<RecoveryFact>,
    /// The revision this answer describes.
    pub revision: u64,
    /// The environment revision it was admitted under.
    pub selected_environment_revision: String,
    /// Why it ended, when it has.
    pub terminal_failure: Option<TerminalFailure>,
    /// The workflow it belongs to, when it belongs to one.
    pub workflow_correlation_identifier: Option<String>,
}

/// What one operation produced, or why it produced nothing.
///
/// Closed, and its variants carry exactly what the row holds. A caller reading
/// `Pending` knows the work has not ended; one reading `RecoveryRequired` knows
/// what is outstanding and how sure the daemon is; one reading `Failed` gets
/// the kind and the one disposition that kind admits. None of them requires
/// reading a message to find out which situation this is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperationResult {
    /// The work has not ended and is not waiting on anything.
    Pending {
        /// The state it has reached.
        lifecycle_state: OperationLifecycleState,
    },
    /// The work has not ended and is waiting on something.
    RecoveryRequired {
        /// What is outstanding, and how sure the daemon is.
        recovery: RecoveryFact,
    },
    /// The work ended without succeeding.
    Failed {
        /// Why it ended, as a pairing the domain validates.
        failure: TerminalFailure,
    },
    /// The work succeeded, and this says where its result is.
    Succeeded {
        /// Where the result went.
        disposition: ResultDisposition,
    },
}

/// Reason a query could not be answered.
#[derive(Debug, thiserror::Error)]
pub enum QueryFailure {
    /// There is no such operation in that partition.
    #[error("no operation named {identifier} in that target partition")]
    NoSuchOperation {
        /// The identifier that was asked about.
        identifier: String,
    },
    /// A terminal row holds a pairing the domain does not admit.
    ///
    /// Reported rather than repaired. A row like this reached the table around
    /// the code that validates pairings, and answering with it would spread
    /// whatever went wrong to every client that asked.
    #[error("the stored terminal failure pairs {kind:?} with a disposition it does not admit")]
    TerminalPairingNotAdmitted {
        /// The kind the row holds.
        kind: slingshot_domain::operation::TerminalFailureKind,
    },
    /// A terminal success holds no record of where its result went.
    #[error("the operation succeeded, and the row does not say where its result went")]
    ResultDispositionMissing,
    /// The database refused.
    #[error(transparent)]
    Repository(#[from] RepositoryFailure),
}

/// Returns what one operation is doing.
///
/// # Errors
///
/// Returns [`QueryFailure::NoSuchOperation`] when that partition holds no such
/// row, or a repository failure.
pub fn status(
    repository: &OperationRepository,
    author_target_identity_digest: &str,
    operation_identifier: &str,
) -> Result<OperationStatus, QueryFailure> {
    let summary = required(repository, author_target_identity_digest, operation_identifier)?;
    Ok(OperationStatus {
        author_target_identity_digest: summary.author_target_identity_digest,
        latest_progress: summary.record.latest_progress,
        lifecycle_state: summary.record.lifecycle_state,
        operation_identifier: summary.operation_identifier,
        outstanding_recovery: summary.record.outstanding_recovery,
        revision: summary.record.revision,
        selected_environment_revision: summary.selected_environment_revision,
        terminal_failure: summary.record.terminal_failure,
        workflow_correlation_identifier: summary.workflow_correlation_identifier,
    })
}

/// Returns what one operation produced, or why it produced nothing.
///
/// # Errors
///
/// Returns [`QueryFailure::NoSuchOperation`],
/// [`QueryFailure::TerminalPairingNotAdmitted`] for a row that reached the
/// table around the domain, [`QueryFailure::ResultDispositionMissing`], or a
/// repository failure.
pub fn result(
    repository: &OperationRepository,
    author_target_identity_digest: &str,
    operation_identifier: &str,
) -> Result<OperationResult, QueryFailure> {
    let summary = required(repository, author_target_identity_digest, operation_identifier)?;
    if let Some(failure) = summary.record.terminal_failure {
        if !terminal_pairing_is_legal(failure.kind, failure.disposition) {
            return Err(QueryFailure::TerminalPairingNotAdmitted { kind: failure.kind });
        }
        return Ok(OperationResult::Failed { failure });
    }
    if summary.record.lifecycle_state == OperationLifecycleState::Succeeded {
        let disposition =
            summary.result_disposition.ok_or(QueryFailure::ResultDispositionMissing)?;
        return Ok(OperationResult::Succeeded { disposition });
    }
    match summary.record.outstanding_recovery {
        Some(recovery) => Ok(OperationResult::RecoveryRequired { recovery }),
        None => Ok(OperationResult::Pending { lifecycle_state: summary.record.lifecycle_state }),
    }
}

/// Reads one operation that a caller asked about by name.
fn required(
    repository: &OperationRepository,
    author_target_identity_digest: &str,
    operation_identifier: &str,
) -> Result<OperationSummary, QueryFailure> {
    repository.read(author_target_identity_digest, operation_identifier)?.ok_or_else(|| {
        QueryFailure::NoSuchOperation { identifier: operation_identifier.to_owned() }
    })
}

/// A sequence above every sequence, for a page that starts at the newest row.
pub const NEWEST_FIRST: u64 = u64::MAX;

/// Where the next page of a listing resumes.
///
/// The arrival sequence rather than a timestamp, because a timestamp is not a
/// position: two operations can share one and a clock can move. A sequence a
/// row was given when it was admitted never changes, so a boundary named by it
/// stays where it was however much work arrives afterwards.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageCursor {
    /// The sequence the next page starts below.
    pub before_enqueue_sequence: u64,
}

/// One page of a target's operations, and where the next one resumes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationPage {
    /// Where the next page resumes, when there is one.
    pub next: Option<PageCursor>,
    /// The rows, newest first.
    pub rows: Vec<OperationListing>,
}

/// Returns one page of `author_target_identity_digest`'s operations.
///
/// A next cursor is offered only when the page filled, so a caller stops when
/// it gets a short page rather than making one more request to discover there
/// is nothing left.
///
/// # Errors
///
/// Returns [`QueryFailure::Repository`] when the database refuses or a stored
/// value does not decode.
pub fn list(
    repository: &OperationRepository,
    author_target_identity_digest: &str,
    cursor: PageCursor,
    page_size: u64,
) -> Result<OperationPage, QueryFailure> {
    let rows = slingshot_storage::operation::listing::list(
        repository.database(),
        author_target_identity_digest,
        cursor.before_enqueue_sequence,
        page_size,
    )?;
    let full = u64::try_from(rows.len()).unwrap_or_default() == page_size;
    let next = full
        .then(|| {
            rows.last().map(|row| PageCursor { before_enqueue_sequence: row.enqueue_sequence })
        })
        .flatten();
    Ok(OperationPage { next, rows })
}
