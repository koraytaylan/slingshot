//! Resuming one paused operation, and doing nothing else at all.
//!
//! Two properties run through everything here. A resume makes a durable row
//! eligible and touches no identity: the command fingerprint, the installation,
//! and the target are the same afterwards, so retrying repeats whatever local
//! half was outstanding and never the remote half, which may already have
//! happened. And an exact repeat is answered from a committed receipt, because
//! whether a resume took effect cannot be reconstructed from current state -
//! later progress, another recovery cycle, and settlement all look the same
//! from outside.

use slingshot_daemon::operation_recovery::{
    ResumeRefusal, ResumeRequest, ResumeResponse, resume, source_fingerprint,
};
use slingshot_domain::command_fingerprint::{CommandFingerprint, FingerprintInput};
use slingshot_domain::installation::InstallationIdentifier;
use slingshot_domain::operation::{
    OperationExecutionCertainty, OperationFact, OperationLifecycleState, RecoveryCategory,
    RecoveryExecutionEvidence, RecoveryFact, TerminalFailure, TerminalFailureDisposition,
    TerminalFailureKind,
};
use slingshot_storage::database::{OperationDatabase, RequiredSettings};
use slingshot_storage::operation_repository::{
    AdmissionOutcome, AdmissionRequest, OperationRepository, OperationSummary,
};

/// Bytes one page occupies, from the runtime contract.
const PAGE_BYTES: u64 = 4096;

/// Pages the database may reach, from the runtime contract.
const DATABASE_PAGES: u64 = 262_144;

/// Milliseconds a busy connection waits, from the runtime contract.
const BUSY_TIMEOUT: u64 = 5000;

/// Two-character pairs in a sixty-four-character hexadecimal value.
const DIGEST_PAIRS: usize = 32;

/// Characters a sixty-four-character hexadecimal value has.
const DIGEST_CHARACTERS: usize = 64;

/// One instant, for a test that does not care which.
const NOW: u64 = 1_700_000_000_000;

/// The instant one fixture step after [`NOW`].
const LATER: u64 = NOW + 1;

/// The environment revision these fixtures are admitted under.
const REVISION: &str = "revision-1";

/// A revision no fixture is admitted under.
const OTHER_REVISION: &str = "revision-0";

/// The first partition these fixtures use.
const FIRST_PRINCIPAL: &str = "1d";

/// The operation every fixture resumes.
const OPERATION: &str = "operation-1";

/// Attempts one fixture recovery has already made.
const SECOND_ATTEMPT: u32 = 2;

/// A revision one source-fingerprint vector resumes from.
const RESUMED_FROM: u64 = 2;

/// A later revision another source-fingerprint vector resumes from.
const RESUMED_LATER: u64 = RESUMED_FROM + 1;

/// Returns the settings every connection is held to.
fn settings() -> RequiredSettings {
    RequiredSettings {
        page_bytes: PAGE_BYTES,
        database_pages: DATABASE_PAGES,
        busy_timeout_milliseconds: BUSY_TIMEOUT,
    }
}

/// Returns one repository over a database held in memory.
fn repository() -> OperationRepository {
    OperationRepository::new(OperationDatabase::open_in_memory(settings()).expect("a database"))
}

/// Returns the digest one principal's author target has.
fn partition(principal: &str) -> String {
    principal.repeat(DIGEST_PAIRS)
}

/// Admits the fixture operation and returns it.
fn admitted(repository: &OperationRepository, digest: &str) -> OperationSummary {
    let canonical = "{\"paths\":[\"/content\"]}";
    let asked = AdmissionRequest {
        author_target_identity: format!("opaque-identity-behind-{digest}"),
        author_target_identity_digest: digest.to_owned(),
        caller_identity: None,
        canonical_command: canonical.to_owned(),
        command_fingerprint: CommandFingerprint::derive(&FingerprintInput {
            author_target_identity_digest: digest.to_owned(),
            canonical_command: canonical.to_owned(),
            command_wire_name: "query_paths".to_owned(),
            command_semantic_contract_version: "1".to_owned(),
            selected_environment_revision: REVISION.to_owned(),
        })
        .expect("a derivable fingerprint"),
        command_wire_name: "query_paths".to_owned(),
        daemon_runtime_contract_digest: "c".repeat(DIGEST_CHARACTERS),
        installation_identifier: InstallationIdentifier::parse(&"a1".repeat(DIGEST_PAIRS))
            .expect("a legal identifier"),
        operation_identifier: OPERATION.to_owned(),
        selected_environment_revision: REVISION.to_owned(),
        workflow_correlation_identifier: None,
    };
    let outcome = repository.admit(&asked, NOW).expect("an admission");
    assert!(matches!(outcome, AdmissionOutcome::Admitted(_)), "the fixture operation admits");
    outcome.summary().clone()
}

/// Pauses the fixture operation on `category`, and returns it.
fn paused(
    repository: &OperationRepository,
    digest: &str,
    category: RecoveryCategory,
    manual_resume_eligible: bool,
) -> OperationSummary {
    let first = admitted(repository, digest);
    let evidence = if matches!(
        category,
        RecoveryCategory::ResultAcquisition
            | RecoveryCategory::ArtifactTransfer
            | RecoveryCategory::PersistentCapacityUnavailable
    ) {
        RecoveryExecutionEvidence::AuthoritativeRemoteSuccess
    } else {
        RecoveryExecutionEvidence::ExecutionCertainty {
            certainty: OperationExecutionCertainty::SubmissionUnknown,
        }
    };
    repository
        .apply(
            digest,
            OPERATION,
            first.record.revision,
            &OperationFact::Recovery {
                recovery: RecoveryFact {
                    attempt_count: 1,
                    category,
                    detail: "outstanding".to_owned(),
                    evidence,
                    manual_resume_eligible,
                    retry_delay_milliseconds: 0,
                    retry_observed_at_unix_milliseconds: NOW,
                },
            },
            NOW,
        )
        .expect("a recovery fact")
}

/// Returns one resume request for `summary`.
fn request(summary: &OperationSummary, category: RecoveryCategory) -> ResumeRequest {
    ResumeRequest {
        author_target_identity_digest: summary.author_target_identity_digest.clone(),
        expected_recovery_category: category,
        expected_revision: summary.record.revision,
        operation_identifier: OPERATION.to_owned(),
        selected_environment_revision: REVISION.to_owned(),
    }
}

#[test]
fn an_exact_resume_commits_one_receipt_and_touches_no_identity() {
    let repository = repository();
    let digest = partition(FIRST_PRINCIPAL);
    let waiting =
        paused(&repository, &digest, RecoveryCategory::PersistentCapacityUnavailable, true);
    let before = repository.read(&digest, OPERATION).expect("a read").expect("a row");

    let response = resume(
        &repository,
        &request(&waiting, RecoveryCategory::PersistentCapacityUnavailable),
        NOW,
    )
    .expect("a resume");
    let ResumeResponse::Applied(receipt) = response else {
        panic!("an exact request resumes: {response:?}");
    };
    assert_eq!(receipt.applied_operation_revision, waiting.record.revision);
    assert_eq!(receipt.selected_environment_revision, REVISION);
    assert_eq!(receipt.operation_identifier, OPERATION);

    let after = repository.read(&digest, OPERATION).expect("a read").expect("a row");
    assert_eq!(
        after.command_fingerprint, before.command_fingerprint,
        "so a retry repeats the local half and never the remote one"
    );
    assert_eq!(after.installation_identifier, before.installation_identifier);
    assert_eq!(after.author_target_identity_digest, before.author_target_identity_digest);
    assert!(!after.record.lifecycle_state.is_terminal(), "and the operation still has not ended");
}

#[test]
fn an_exact_repeat_replays_after_progress_a_new_cycle_and_settlement() {
    let repository = repository();
    let digest = partition(FIRST_PRINCIPAL);
    let waiting = paused(&repository, &digest, RecoveryCategory::AmbiguousSubmission, true);
    let asked = request(&waiting, RecoveryCategory::AmbiguousSubmission);

    let ResumeResponse::Applied(receipt) = resume(&repository, &asked, NOW).expect("a resume")
    else {
        panic!("the first request resumes");
    };

    let advanced = repository
        .apply(
            &digest,
            OPERATION,
            waiting.record.revision,
            &OperationFact::Progress { detail: "trying again".to_owned() },
            LATER,
        )
        .expect("later progress");
    let cycled = repository
        .apply(
            &digest,
            OPERATION,
            advanced.record.revision,
            &OperationFact::Recovery {
                recovery: RecoveryFact {
                    attempt_count: SECOND_ATTEMPT,
                    category: RecoveryCategory::OperationLookup,
                    detail: "still outstanding".to_owned(),
                    evidence: RecoveryExecutionEvidence::ExecutionCertainty {
                        certainty: OperationExecutionCertainty::RemoteOutcomeUnknown,
                    },
                    manual_resume_eligible: true,
                    retry_delay_milliseconds: 0,
                    retry_observed_at_unix_milliseconds: LATER,
                },
            },
            LATER,
        )
        .expect("another recovery cycle");
    repository
        .apply(
            &digest,
            OPERATION,
            cycled.record.revision,
            &OperationFact::Terminal {
                failure: TerminalFailure {
                    disposition: TerminalFailureDisposition::FailClosedIndeterminate {
                        certainty: OperationExecutionCertainty::RemoteOutcomeUnknown,
                    },
                    kind: TerminalFailureKind::RecoveryWindowExpired,
                    metadata: None,
                },
            },
            LATER,
        )
        .expect("a settlement");

    let response = resume(&repository, &asked, LATER).expect("a repeat after everything");
    let ResumeResponse::Replayed(replayed) = response else {
        panic!("an exact repeat replays whatever happened since: {response:?}");
    };
    assert_eq!(replayed, receipt, "and hands back exactly what was committed");
}

#[test]
fn a_repeat_replays_across_a_reopen() {
    let directory = tempfile::tempdir().expect("a directory");
    let path = directory.path().join("operations.sqlite3");
    let digest = partition(FIRST_PRINCIPAL);
    let (asked, receipt) = {
        let repository = OperationRepository::new(
            OperationDatabase::open(&path, settings()).expect("a database"),
        );
        let waiting = paused(&repository, &digest, RecoveryCategory::ArtifactTransfer, true);
        let asked = request(&waiting, RecoveryCategory::ArtifactTransfer);
        let ResumeResponse::Applied(receipt) = resume(&repository, &asked, NOW).expect("a resume")
        else {
            panic!("the first request resumes");
        };
        (asked, receipt)
    };

    let repository = OperationRepository::new(
        OperationDatabase::open(&path, settings()).expect("a reopened database"),
    );
    let response = resume(&repository, &asked, LATER).expect("a repeat after a restart");
    let ResumeResponse::Replayed(replayed) = response else {
        panic!("a receipt outlives the process that wrote it: {response:?}");
    };
    assert_eq!(replayed, receipt);
}

#[test]
fn every_wrong_precondition_refuses_and_leaves_the_operation_alone() {
    let repository = repository();
    let digest = partition(FIRST_PRINCIPAL);
    let waiting = paused(&repository, &digest, RecoveryCategory::AmbiguousSubmission, true);
    let before = repository.read(&digest, OPERATION).expect("a read").expect("a row");

    let wrong_category = ResumeRequest {
        expected_recovery_category: RecoveryCategory::ResultAcquisition,
        ..request(&waiting, RecoveryCategory::AmbiguousSubmission)
    };
    assert!(
        matches!(
            resume(&repository, &wrong_category, NOW).expect("a response"),
            ResumeResponse::Refused(ResumeRefusal::CategoryMismatch { .. })
        ),
        "a person resuming the wrong thing is refused rather than resuming the right one"
    );

    let stale = ResumeRequest {
        expected_revision: waiting.record.revision - 1,
        ..request(&waiting, RecoveryCategory::AmbiguousSubmission)
    };
    assert!(
        matches!(
            resume(&repository, &stale, NOW).expect("a response"),
            ResumeResponse::Refused(ResumeRefusal::RevisionMoved { .. })
        ),
        "and so is one acting on a revision that has moved"
    );

    let other_revision = ResumeRequest {
        selected_environment_revision: OTHER_REVISION.to_owned(),
        ..request(&waiting, RecoveryCategory::AmbiguousSubmission)
    };
    assert!(
        matches!(
            resume(&repository, &other_revision, NOW).expect("a response"),
            ResumeResponse::Refused(ResumeRefusal::RevisionMismatch)
        ),
        "and one from another environment revision"
    );

    let missing = ResumeRequest {
        operation_identifier: "operation-nobody-admitted".to_owned(),
        ..request(&waiting, RecoveryCategory::AmbiguousSubmission)
    };
    assert!(
        matches!(
            resume(&repository, &missing, NOW).expect("a response"),
            ResumeResponse::Refused(ResumeRefusal::NoSuchOperation { .. })
        ),
        "and one naming nothing"
    );

    assert_eq!(
        repository.read(&digest, OPERATION).expect("a read").expect("a row"),
        before,
        "and every refusal left the operation byte for byte as it was"
    );
}

#[test]
fn an_ended_operation_and_a_running_one_are_both_refused() {
    let repository = repository();
    let digest = partition(FIRST_PRINCIPAL);
    let running = admitted(&repository, &digest);
    assert!(
        matches!(
            resume(&repository, &request(&running, RecoveryCategory::AmbiguousSubmission), NOW)
                .expect("a response"),
            ResumeResponse::Refused(ResumeRefusal::NotWaiting)
        ),
        "an operation waiting on nothing has nothing to resume"
    );

    let settled = repository
        .apply(
            &digest,
            OPERATION,
            running.record.revision,
            &OperationFact::Terminal {
                failure: TerminalFailure {
                    disposition: TerminalFailureDisposition::AuthoritativeRemoteFailure,
                    kind: TerminalFailureKind::RemoteFailed,
                    metadata: None,
                },
            },
            NOW,
        )
        .expect("a settlement");
    assert_eq!(settled.record.lifecycle_state, OperationLifecycleState::Failed);
    assert!(
        matches!(
            resume(&repository, &request(&settled, RecoveryCategory::AmbiguousSubmission), NOW)
                .expect("a response"),
            ResumeResponse::Refused(ResumeRefusal::AlreadyTerminal)
        ),
        "and an ending is not something to resume"
    );
}

#[test]
fn a_recovery_the_daemon_retries_itself_is_not_one_a_person_resumes() {
    let repository = repository();
    let digest = partition(FIRST_PRINCIPAL);
    let waiting = paused(&repository, &digest, RecoveryCategory::EventReconnection, false);

    let response =
        resume(&repository, &request(&waiting, RecoveryCategory::EventReconnection), NOW)
            .expect("a response");
    assert!(
        matches!(response, ResumeResponse::Refused(ResumeRefusal::NotManuallyResumable)),
        "the daemon comes back to this on its own, so a person asking is told so: {response:?}"
    );
    assert!(
        repository
            .read_resume_receipt(
                &digest,
                &source_fingerprint(
                    waiting.command_fingerprint.as_text(),
                    waiting.record.revision,
                    RecoveryCategory::EventReconnection,
                ),
            )
            .expect("a read")
            .is_none(),
        "and no receipt was written for something that did not happen"
    );
}

#[test]
fn two_resumes_of_one_operation_at_different_points_are_two_sources() {
    let fingerprint = "f".repeat(DIGEST_CHARACTERS);
    let first =
        source_fingerprint(&fingerprint, RESUMED_FROM, RecoveryCategory::AmbiguousSubmission);
    let same =
        source_fingerprint(&fingerprint, RESUMED_FROM, RecoveryCategory::AmbiguousSubmission);
    let later =
        source_fingerprint(&fingerprint, RESUMED_LATER, RecoveryCategory::AmbiguousSubmission);
    let other = source_fingerprint(&fingerprint, RESUMED_FROM, RecoveryCategory::OperationLookup);

    assert_eq!(first, same, "the same resume sent twice is one source");
    assert_ne!(first, later, "resuming from a later revision is another");
    assert_ne!(first, other, "and resuming another category is another again");
}
