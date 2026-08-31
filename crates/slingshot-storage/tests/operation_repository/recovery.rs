//! Recovery facts, terminal settlement, and durable resume receipts.
//!
//! The pairing rules are the domain's, and these prove the database keeps
//! them: a category carries only the evidence it admits, a terminal kind only
//! the disposition it admits, and a row that reached the table around the
//! domain is refused on the way back rather than decoded into something
//! plausible.

use slingshot_domain::operation::{
    LifecycleFailure, OperationExecutionCertainty, OperationFact, OperationLifecycleState,
    RecoveryCategory, RecoveryExecutionEvidence, TerminalFailure, TerminalFailureDisposition,
    TerminalFailureKind,
};
use slingshot_storage::database::OperationDatabase;
use slingshot_storage::operation_repository::{
    OperationRepository, RepositoryFailure, ResumeOutcome,
};

use crate::fixtures::*;

#[test]
fn every_recovery_category_survives_a_reopen_with_the_evidence_it_admits() {
    let unresolved = RecoveryExecutionEvidence::ExecutionCertainty {
        certainty: OperationExecutionCertainty::SubmissionUnknown,
    };
    let proven = RecoveryExecutionEvidence::AuthoritativeRemoteSuccess;
    let pairs = [
        (RecoveryCategory::AmbiguousSubmission, unresolved),
        (RecoveryCategory::EventReconnection, unresolved),
        (RecoveryCategory::OperationLookup, unresolved),
        (RecoveryCategory::JobSnapshotRecovery, unresolved),
        (RecoveryCategory::ResultAcquisition, proven),
        (RecoveryCategory::ArtifactTransfer, proven),
        (RecoveryCategory::PersistentCapacityUnavailable, proven),
    ];

    let directory = tempfile::tempdir().expect("a directory");
    let path = directory.path().join("operations.sqlite3");
    for (index, (category, evidence)) in pairs.iter().enumerate() {
        let digest = format!("{index:02}").repeat(DIGEST_PAIRS);
        let recorded = {
            let store = repository(&path);
            let first = admitted(&store, &digest);
            let fact = recovering(*category, *evidence, ATTEMPTS_ALREADY_MADE);
            applied(&store, &digest, first, &fact, NOW)
        };
        let store = repository(&path);
        let reopened = store.read(&digest, OPERATION).expect("a read").expect("a row");
        assert_eq!(reopened, recorded, "{category:?} comes back exactly as it went in");
        let held = reopened.record.outstanding_recovery.expect("an outstanding recovery");
        assert_eq!(held.category, *category);
        assert_eq!(held.evidence, *evidence, "{category:?} carries the evidence it admits");
        assert_eq!(held.attempt_count, ATTEMPTS_ALREADY_MADE, "and the attempts already made");
        assert_eq!(held.retry_delay_milliseconds, RETRY_DELAY_MILLISECONDS, "and how long to wait");
        assert_eq!(held.retry_observed_at_unix_milliseconds, NOW, "and when the wait started");
        assert!(held.manual_resume_eligible, "and whether a person may resume it");
    }
}

#[test]
fn a_recovery_category_cannot_be_stored_with_evidence_it_does_not_admit() {
    let store = in_memory();
    let digest = partition(FIRST_PRINCIPAL);
    let first = admitted(&store, &digest);

    let proven_success_with_a_doubt = OperationFact::Recovery {
        recovery: recovery(
            RecoveryCategory::ResultAcquisition,
            RecoveryExecutionEvidence::ExecutionCertainty {
                certainty: OperationExecutionCertainty::RemoteOutcomeUnknown,
            },
            1,
        ),
    };
    let refused = store.apply(&digest, OPERATION, first, &proven_success_with_a_doubt, NOW);
    assert!(
        matches!(refused, Err(RepositoryFailure::Lifecycle(LifecycleFailure::EvidenceNotAdmitted))),
        "retrieving a proven result carries no doubt that it happened: {refused:?}"
    );
    assert!(
        store
            .read(&digest, OPERATION)
            .expect("a read")
            .expect("a row")
            .record
            .outstanding_recovery
            .is_none(),
        "and nothing was written"
    );
}

#[test]
fn settling_an_operation_clears_the_recovery_it_was_waiting_on() {
    let store = in_memory();
    let digest = partition(FIRST_PRINCIPAL);
    let first = admitted(&store, &digest);

    let waiting = OperationFact::Recovery {
        recovery: recovery(
            RecoveryCategory::ResultAcquisition,
            RecoveryExecutionEvidence::AuthoritativeRemoteSuccess,
            ATTEMPTS_ALREADY_MADE,
        ),
    };
    let paused = applied(&store, &digest, first, &waiting, NOW);
    assert!(paused.record.outstanding_recovery.is_some(), "it is waiting on something");

    let ended = OperationFact::Terminal {
        failure: TerminalFailure {
            disposition: TerminalFailureDisposition::AuthoritativeRemoteSuccess,
            kind: TerminalFailureKind::ResultUnavailable,
            metadata: Some("the artifact never arrived".to_owned()),
        },
    };
    let settled =
        applied(&store, &digest, paused.record.revision, &ended, NOW + SETTLING_DELAY_MILLISECONDS);
    assert_eq!(settled.record.lifecycle_state, OperationLifecycleState::Failed);
    assert!(
        settled.record.outstanding_recovery.is_none(),
        "an operation that has ended is waiting on nothing"
    );
    assert_eq!(
        settled.settled_at_unix_milliseconds,
        Some(NOW + SETTLING_DELAY_MILLISECONDS),
        "and records when it ended"
    );
}

#[test]
fn every_legal_terminal_pairing_decodes_back_as_the_one_it_is() {
    let confirmed_not_executed = TerminalFailureDisposition::AuthoritativeNonExecution {
        certainty: OperationExecutionCertainty::ConfirmedNotExecuted,
    };
    let indeterminate = TerminalFailureDisposition::FailClosedIndeterminate {
        certainty: OperationExecutionCertainty::RemoteOutcomeUnknown,
    };
    let pairings = [
        (TerminalFailureKind::Rejected, confirmed_not_executed),
        (TerminalFailureKind::RemoteFailed, TerminalFailureDisposition::AuthoritativeRemoteFailure),
        (
            TerminalFailureKind::ResultUnavailable,
            TerminalFailureDisposition::AuthoritativeRemoteSuccess,
        ),
        (TerminalFailureKind::RecoveryWindowExpired, indeterminate),
        (TerminalFailureKind::RemoteStateLost, indeterminate),
        (TerminalFailureKind::IntegrityFailure, indeterminate),
        (TerminalFailureKind::RetryPolicyExhausted, confirmed_not_executed),
    ];
    let directory = tempfile::tempdir().expect("a directory");
    let path = directory.path().join("operations.sqlite3");
    for (index, (kind, disposition)) in pairings.iter().enumerate() {
        let digest = format!("{index:02}").repeat(DIGEST_PAIRS);
        let failure = TerminalFailure { disposition: *disposition, kind: *kind, metadata: None };
        {
            let store = repository(&path);
            let first = admitted(&store, &digest);
            let fact = OperationFact::Terminal { failure: failure.clone() };
            applied(&store, &digest, first, &fact, NOW);
        }
        let store = repository(&path);
        let reopened = store.read(&digest, OPERATION).expect("a read").expect("a row");
        assert_eq!(
            reopened.record.terminal_failure,
            Some(failure),
            "{kind:?} comes back paired with the disposition it was settled under"
        );
    }
}

#[test]
fn a_terminal_pairing_the_domain_refuses_never_reaches_a_row() {
    let store = in_memory();
    let digest = partition(FIRST_PRINCIPAL);
    let first = admitted(&store, &digest);
    let proven_work_called_never_attempted = OperationFact::Terminal {
        failure: TerminalFailure {
            disposition: TerminalFailureDisposition::AuthoritativeRemoteSuccess,
            kind: TerminalFailureKind::RecoveryWindowExpired,
            metadata: None,
        },
    };
    let refused = store.apply(&digest, OPERATION, first, &proven_work_called_never_attempted, NOW);
    assert!(
        matches!(
            refused,
            Err(RepositoryFailure::Lifecycle(LifecycleFailure::DispositionNotAdmitted))
        ),
        "work the remote provably did cannot be recorded as work that ran out of time: {refused:?}"
    );
    let held = store.read(&digest, OPERATION).expect("a read").expect("a row");
    assert_eq!(held.record.revision, first, "and the operation is where it was");
    assert!(held.record.terminal_failure.is_none(), "with no terminal fact at all");
    assert!(held.settled_at_unix_milliseconds.is_none(), "and no settlement instant");
}

#[test]
fn the_stored_terminal_disposition_is_the_domain_s_own_encoding_and_nothing_more() {
    let directory = tempfile::tempdir().expect("a directory");
    let path = directory.path().join("operations.sqlite3");
    let digest = partition(FIRST_PRINCIPAL);
    {
        let store = repository(&path);
        let first = admitted(&store, &digest);
        let fact = settling(
            TerminalFailureKind::RemoteStateLost,
            TerminalFailureDisposition::FailClosedIndeterminate {
                certainty: OperationExecutionCertainty::RemoteOutcomeUnknown,
            },
        );
        applied(&store, &digest, first, &fact, NOW);
    }

    let database = OperationDatabase::open(&path, settings()).expect("a database");
    let (kind, disposition): (String, String) = database
        .connection()
        .query_row(
            "SELECT terminal_failure_kind, terminal_failure_disposition FROM operation \
             WHERE author_target_identity_digest = ? AND operation_identifier = ?",
            rusqlite::params![digest, OPERATION],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("the settled row");
    assert_eq!(kind, "remote_state_lost", "the kind is the word the domain spells");
    assert_eq!(
        disposition,
        "{\"disposition\":\"fail_closed_indeterminate\",\"certainty\":\"remote_outcome_unknown\"}",
        "and the payload is the disposition and its certainty, with nothing else beside them"
    );
}

#[test]
fn a_row_that_bypassed_the_domain_is_refused_rather_than_decoded() {
    let directory = tempfile::tempdir().expect("a directory");
    let path = directory.path().join("operations.sqlite3");
    let digest = partition(FIRST_PRINCIPAL);
    {
        let store = repository(&path);
        admitted(&store, &digest);
    }
    let database = OperationDatabase::open(&path, settings()).expect("a database");
    database
        .connection()
        .execute(
            "INSERT INTO recovery_fact \
             (attempt_count, author_target_identity_digest, category, evidence_certainty, \
              evidence_kind, manual_resume_eligible, operation_identifier, \
              retry_delay_milliseconds, retry_observed_at_unix_milliseconds) \
             VALUES (1, ?, 'not_a_category', NULL, 'authoritative_remote_success', 0, ?, 0, 0)",
            rusqlite::params![digest, OPERATION],
        )
        .expect("a row written around the repository");

    let store = OperationRepository::new(database);
    let read = store.read(&digest, OPERATION);
    assert!(
        matches!(read, Err(RepositoryFailure::NotDecodable { column: "category", .. })),
        "a category this daemon does not have is refused, not guessed at: {read:?}"
    );
}

#[test]
fn one_resume_source_commits_one_receipt_and_replays_it_afterwards() {
    let directory = tempfile::tempdir().expect("a directory");
    let path = directory.path().join("operations.sqlite3");
    let digest = partition(FIRST_PRINCIPAL);
    let applied = {
        let store = repository(&path);
        let first = admitted(&store, &digest);
        let outcome = receipted(&store, &digest, 1, first + 1, NOW);
        let ResumeOutcome::Applied(receipt) = outcome else {
            panic!("a source with no receipt applies one");
        };
        assert_eq!(receipt.applied_operation_revision, first + 1);
        assert_eq!(receipt.recorded_at_unix_milliseconds, NOW);
        assert_eq!(receipt.selected_environment_revision, "revision-1");
        *receipt
    };

    let store = repository(&path);
    let outcome = receipted(&store, &digest, 1, REVISION_NOBODY_IS_AT, SECOND_INSTANT);
    let ResumeOutcome::Replayed(replayed) = outcome else {
        panic!("a source that already has a receipt replays it");
    };
    assert_eq!(
        replayed.as_ref(),
        &applied,
        "the committed receipt, not a fresh one built from what was asked"
    );
    assert_eq!(
        store.read_resume_receipt(&digest, &source(1)).expect("a read").as_ref(),
        Some(&applied),
        "and the receipt reads back the same either way"
    );
}

#[test]
fn a_receipt_outlives_every_revision_the_operation_goes_through_afterwards() {
    let directory = tempfile::tempdir().expect("a directory");
    let path = directory.path().join("operations.sqlite3");
    let digest = partition(FIRST_PRINCIPAL);
    let committed = {
        let store = repository(&path);
        let first = admitted(&store, &digest);
        let outcome = receipted(&store, &digest, 1, first, NOW);
        let advance = reaching(OperationLifecycleState::Submitting);
        let progressed = applied(&store, &digest, first, &advance, NOW);
        let cycle = recovering(RecoveryCategory::AmbiguousSubmission, SUBMISSION_UNKNOWN, 1);
        let recovered =
            applied(&store, &digest, progressed.record.revision, &cycle, SECOND_INSTANT);
        let end = settling(
            TerminalFailureKind::RecoveryWindowExpired,
            TerminalFailureDisposition::FailClosedIndeterminate {
                certainty: OperationExecutionCertainty::SubmissionUnknown,
            },
        );
        applied(&store, &digest, recovered.record.revision, &end, THIRD_INSTANT);
        outcome
    };
    let ResumeOutcome::Applied(receipt) = committed else { panic!("the first call applies") };

    let store = repository(&path);
    let outcome = receipted(&store, &digest, 1, 1, FOURTH_INSTANT);
    let ResumeOutcome::Replayed(replayed) = outcome else {
        panic!("later progress, another recovery cycle, and settlement do not hide a receipt");
    };
    assert_eq!(replayed, receipt, "and it replays exactly what was committed");
}

#[test]
fn one_operation_holds_the_receipts_it_may_and_refuses_the_next() {
    let store = in_memory();
    let digest = partition(FIRST_PRINCIPAL);
    admitted(&store, &digest);
    let allowed = usize::try_from(RECEIPTS_PER_OPERATION).expect("a countable bound");

    for index in 0..allowed {
        let outcome = receipted(&store, &digest, index, 1, NOW);
        assert!(matches!(outcome, ResumeOutcome::Applied(_)), "each fresh source applies one");
    }

    let over =
        store.record_resume_receipt(&digest, OPERATION, &source(allowed), "revision-1", 1, NOW);
    assert!(
        matches!(over, Err(RepositoryFailure::ReceiptsExhausted { allowed: bound })
            if bound == RECEIPTS_PER_OPERATION),
        "one source beyond the bound is refused: {over:?}"
    );
    assert!(
        store.read_resume_receipt(&digest, &source(allowed)).expect("a read").is_none(),
        "and wrote nothing"
    );
    let replay = receipted(&store, &digest, 0, 1, NOW);
    assert!(
        matches!(replay, ResumeOutcome::Replayed(_)),
        "while an exact repeat still replays, because it consumes nothing"
    );
}

#[test]
fn concurrent_identical_resume_requests_commit_one_receipt() {
    let directory = tempfile::tempdir().expect("a directory");
    let path = directory.path().join("operations.sqlite3");
    let digest = partition(FIRST_PRINCIPAL);
    admitted(&repository(&path), &digest);

    let raced = digest.clone();
    let committed = race(&path, move |store, _| {
        matches!(receipted(store, &raced, 1, 2, NOW), ResumeOutcome::Applied(_))
    });
    assert_eq!(committed.iter().filter(|won| **won).count(), 1, "one committed the receipt");

    let store = repository(&path);
    assert!(
        store.read_resume_receipt(&digest, &source(1)).expect("a read").is_some(),
        "and it is there afterwards"
    );
}
