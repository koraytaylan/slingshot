//! Which execute requests reach durable state, and what settling one records.
//!
//! Every refusal here is checked twice: once for saying the right thing, and
//! once for having created nothing. That second check is the one that matters,
//! because a refusal that left a row behind gives a client an operation it can
//! find and wait on, describing work nothing was ever going to do.

use slingshot_daemon::operation_submission::{
    ExecuteRequest, ServedTarget, SubmissionOutcome, SubmissionRefusal, capacity_unavailable,
    settle, submit,
};
use slingshot_domain::command_fingerprint::{CommandFingerprint, FingerprintInput};
use slingshot_domain::installation::InstallationIdentifier;
use slingshot_domain::operation::{
    OperationExecutionCertainty, OperationLifecycleState, RecoveryCategory,
    RecoveryExecutionEvidence, TerminalFailure, TerminalFailureDisposition, TerminalFailureKind,
};
use slingshot_domain::operation_executor::OperationExecutorOutcome;
use slingshot_storage::database::{OperationDatabase, RequiredSettings};
use slingshot_storage::operation_repository::{
    AdmissionRequest, OperationRepository, ResultDisposition,
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
const SECOND_INSTANT: u64 = NOW + 1;

/// The instant one fixture step after [`SECOND_INSTANT`].
const THIRD_INSTANT: u64 = SECOND_INSTANT + 1;

/// The environment revision this daemon serves.
const REVISION: &str = "revision-1";

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

/// Returns what this daemon serves.
fn served() -> ServedTarget {
    ServedTarget {
        author_target_identity_digest: "1d".repeat(DIGEST_PAIRS),
        daemon_runtime_contract_digest: "c".repeat(DIGEST_CHARACTERS),
        execution_available: true,
        selected_environment_revision: REVISION.to_owned(),
    }
}

/// Returns one execute request against `digest` at `revision`.
fn request(digest: &str, revision: &str, identifier: &str) -> ExecuteRequest {
    let canonical = format!("{{\"paths\":[\"/{identifier}\"]}}");
    ExecuteRequest {
        admission: AdmissionRequest {
            author_target_identity: format!("opaque-identity-behind-{digest}"),
            author_target_identity_digest: digest.to_owned(),
            caller_identity: Some("caller-1".to_owned()),
            canonical_command: canonical.clone(),
            command_fingerprint: CommandFingerprint::derive(&FingerprintInput {
                author_target_identity_digest: digest.to_owned(),
                canonical_command: canonical,
                command_wire_name: "query_paths".to_owned(),
                command_semantic_contract_version: "1".to_owned(),
                selected_environment_revision: revision.to_owned(),
            })
            .expect("a derivable fingerprint"),
            command_wire_name: "query_paths".to_owned(),
            daemon_runtime_contract_digest: "c".repeat(DIGEST_CHARACTERS),
            installation_identifier: InstallationIdentifier::parse(&"a1".repeat(DIGEST_PAIRS))
                .expect("a legal identifier"),
            operation_identifier: identifier.to_owned(),
            selected_environment_revision: revision.to_owned(),
            workflow_correlation_identifier: None,
        },
        command_semantic_contract_version: "1".to_owned(),
        expected_daemon_runtime_contract_digest: "c".repeat(DIGEST_CHARACTERS),
    }
}

#[test]
fn a_build_that_runs_nothing_admits_nothing_and_says_so_first() {
    let unavailable = ServedTarget { execution_available: false, ..served() };
    let repository = repository();
    let digest = served().author_target_identity_digest;

    let outcome =
        submit(&unavailable, &repository, &request(&digest, REVISION, "operation-1"), NOW)
            .expect("a classification");
    assert_eq!(
        outcome,
        SubmissionOutcome::Refused(SubmissionRefusal::ExecutionUnavailable),
        "availability is settled before anything else, so this answer costs no database access"
    );
    assert!(
        repository.read(&digest, "operation-1").expect("a read").is_none(),
        "and no operation was created"
    );

    let elsewhere = "2d".repeat(DIGEST_PAIRS);
    let outcome = submit(&unavailable, &repository, &request(&elsewhere, REVISION, "x"), NOW)
        .expect("a classification");
    assert_eq!(
        outcome,
        SubmissionOutcome::Refused(SubmissionRefusal::ExecutionUnavailable),
        "and a client asking about another target gets the same answer, because it is the same fact"
    );
}

#[test]
fn a_request_for_something_else_reaches_neither_repository_nor_executor() {
    let repository = repository();
    let mismatches = [
        ("2d".repeat(DIGEST_PAIRS), REVISION.to_owned(), SubmissionRefusal::TargetMismatch),
        (
            served().author_target_identity_digest,
            "revision-0".to_owned(),
            SubmissionRefusal::RevisionMismatch,
        ),
    ];
    for (digest, revision, expected) in mismatches {
        let outcome =
            submit(&served(), &repository, &request(&digest, &revision, "operation-1"), NOW)
                .expect("a classification");
        assert_eq!(outcome, SubmissionOutcome::Refused(expected), "a mismatch is a refusal");
        assert!(
            repository.read(&digest, "operation-1").expect("a read").is_none(),
            "and creates nothing in the partition it named"
        );
    }

    let digest = served().author_target_identity_digest;
    let mut other_contract = request(&digest, REVISION, "operation-1");
    other_contract.expected_daemon_runtime_contract_digest = "f".repeat(DIGEST_CHARACTERS);
    let outcome = submit(&served(), &repository, &other_contract, NOW).expect("a classification");
    assert_eq!(
        outcome,
        SubmissionOutcome::Refused(SubmissionRefusal::ContractMismatch),
        "and so does a client expecting another runtime contract"
    );
    assert!(repository.read(&digest, "operation-1").expect("a read").is_none());
}

#[test]
fn an_admissible_request_is_admitted_once_and_repeats_replay() {
    let repository = repository();
    let digest = served().author_target_identity_digest;
    let asked = request(&digest, REVISION, "operation-1");

    let first = submit(&served(), &repository, &asked, NOW).expect("a first submission");
    let SubmissionOutcome::Admitted(admitted) = first else {
        panic!("a first admissible request admits");
    };
    assert_eq!(admitted.record.lifecycle_state, OperationLifecycleState::Queued);

    let again = submit(&served(), &repository, &asked, SECOND_INSTANT).expect("a repeat");
    assert_eq!(
        again,
        SubmissionOutcome::Replayed(admitted.clone()),
        "and a repeat hands back the row rather than making a second"
    );

    let mut conflicting = request(&digest, REVISION, "operation-1");
    conflicting.admission.canonical_command = "{\"paths\":[\"/elsewhere\"]}".to_owned();
    let outcome =
        submit(&served(), &repository, &conflicting, THIRD_INSTANT).expect("a classification");
    assert!(
        matches!(outcome, SubmissionOutcome::Conflict(_)),
        "while different work under that identifier conflicts: {outcome:?}"
    );
    assert_eq!(
        repository.read(&digest, "operation-1").expect("a read").expect("a row"),
        *admitted,
        "and the conflict changed nothing"
    );
}

#[test]
fn settling_a_success_records_where_the_result_went() {
    let repository = repository();
    let digest = served().author_target_identity_digest;
    let asked = request(&digest, REVISION, "operation-1");
    let SubmissionOutcome::Admitted(admitted) =
        submit(&served(), &repository, &asked, NOW).expect("a submission")
    else {
        panic!("a first request admits");
    };
    let running = repository
        .apply(
            &digest,
            "operation-1",
            admitted.record.revision,
            &slingshot_domain::operation::OperationFact::Lifecycle {
                lifecycle_state: OperationLifecycleState::Submitting,
            },
            NOW,
        )
        .expect("an advance");
    let running = repository
        .apply(
            &digest,
            "operation-1",
            running.record.revision,
            &slingshot_domain::operation::OperationFact::Lifecycle {
                lifecycle_state: OperationLifecycleState::Accepted,
            },
            NOW,
        )
        .expect("an advance");
    let running = repository
        .apply(
            &digest,
            "operation-1",
            running.record.revision,
            &slingshot_domain::operation::OperationFact::Lifecycle {
                lifecycle_state: OperationLifecycleState::Running,
            },
            NOW,
        )
        .expect("an advance");

    let settled = settle(
        &repository,
        &running,
        &OperationExecutorOutcome::Succeeded {
            artifacts: Vec::new(),
            inline_result: Some("{}".to_owned()),
        },
        NOW,
    )
    .expect("a settlement");
    assert_eq!(settled.record.lifecycle_state, OperationLifecycleState::Succeeded);
    assert_eq!(
        settled.result_disposition,
        Some(ResultDisposition::Inline),
        "a small result travels in the response, and the row says so"
    );
    assert!(settled.settled_at_unix_milliseconds.is_some(), "and the operation has ended");
}

#[test]
fn settling_a_terminal_failure_keeps_the_pairing_the_domain_validates() {
    let repository = repository();
    let digest = served().author_target_identity_digest;
    let SubmissionOutcome::Admitted(admitted) =
        submit(&served(), &repository, &request(&digest, REVISION, "operation-1"), NOW)
            .expect("a submission")
    else {
        panic!("a first request admits");
    };

    let settled = settle(
        &repository,
        &admitted,
        &OperationExecutorOutcome::TerminalFailure {
            failure: TerminalFailure {
                disposition: TerminalFailureDisposition::AuthoritativeRemoteSuccess,
                kind: TerminalFailureKind::ResultUnavailable,
                metadata: None,
            },
        },
        NOW,
    )
    .expect("a settlement");
    let failure = settled.record.terminal_failure.expect("a terminal failure");
    assert_eq!(failure.kind, TerminalFailureKind::ResultUnavailable);
    assert_eq!(
        failure.disposition,
        TerminalFailureDisposition::AuthoritativeRemoteSuccess,
        "work the remote provably did is not rewritten as work that might not have happened"
    );
    assert_eq!(settled.result_disposition, None, "and nothing was published");
}

#[test]
fn a_result_that_cannot_be_stored_leaves_proven_work_nonterminal() {
    let repository = repository();
    let digest = served().author_target_identity_digest;
    let SubmissionOutcome::Admitted(admitted) =
        submit(&served(), &repository, &request(&digest, REVISION, "operation-1"), NOW)
            .expect("a submission")
    else {
        panic!("a first request admits");
    };
    let fingerprint = admitted.command_fingerprint.clone();

    let waiting = settle(
        &repository,
        &admitted,
        &OperationExecutorOutcome::RecoveryRequired {
            recovery: capacity_unavailable("the result does not fit", 1, NOW),
        },
        NOW,
    )
    .expect("a settlement");

    assert!(!waiting.record.lifecycle_state.is_terminal(), "the operation has not ended");
    let outstanding = waiting.record.outstanding_recovery.expect("a recovery fact");
    assert_eq!(outstanding.category, RecoveryCategory::PersistentCapacityUnavailable);
    assert_eq!(
        outstanding.evidence,
        RecoveryExecutionEvidence::AuthoritativeRemoteSuccess,
        "and the record says the remote succeeded rather than inventing a doubt"
    );
    assert!(outstanding.manual_resume_eligible, "capacity is something a person can go and free");
    assert_eq!(waiting.result_disposition, None, "no result slot was published");
    assert_eq!(
        waiting.command_fingerprint, fingerprint,
        "and the identity is untouched, so a resume retries the local half and not the remote one"
    );
    assert_ne!(
        outstanding.evidence,
        RecoveryExecutionEvidence::ExecutionCertainty {
            certainty: OperationExecutionCertainty::RemoteOutcomeUnknown
        },
        "which is the mistake this case exists to rule out"
    );
}

#[test]
fn the_daemon_fingerprints_rather_than_believing_what_it_was_handed() {
    let repository = repository();
    let digest = served().author_target_identity_digest;

    let mut lying = request(&digest, REVISION, "operation-1");
    lying.admission.command_fingerprint =
        CommandFingerprint::parse(&"f".repeat(DIGEST_CHARACTERS)).expect("a legal fingerprint");
    let SubmissionOutcome::Admitted(admitted) =
        submit(&served(), &repository, &lying, NOW).expect("a submission")
    else {
        panic!("an admissible request admits whatever fingerprint it claimed");
    };
    assert_ne!(
        admitted.command_fingerprint, lying.admission.command_fingerprint,
        "the stored fingerprint is the one this daemon derived"
    );
    assert_eq!(
        admitted.command_fingerprint,
        request(&digest, REVISION, "operation-1").admission.command_fingerprint,
        "which is what the command, target, and revision actually produce"
    );

    let honest = request(&digest, REVISION, "operation-1");
    assert!(
        matches!(
            submit(&served(), &repository, &honest, SECOND_INSTANT).expect("a repeat"),
            SubmissionOutcome::Replayed(_)
        ),
        "so the same work replays however either request spelled its fingerprint"
    );

    let mut also_lying = request(&digest, REVISION, "operation-1");
    also_lying.admission.canonical_command = "{\"paths\":[\"/elsewhere\"]}".to_owned();
    also_lying.admission.command_fingerprint = admitted.command_fingerprint.clone();
    assert!(
        matches!(
            submit(&served(), &repository, &also_lying, THIRD_INSTANT).expect("a classification"),
            SubmissionOutcome::Conflict(_)
        ),
        "and different work cannot pass itself off as a repeat by claiming the right fingerprint"
    );
}
