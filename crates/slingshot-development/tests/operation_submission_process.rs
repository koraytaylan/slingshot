//! Submitting through the composition each binary actually has.
//!
//! The product binary composes an executor that runs nothing, so every execute
//! it receives is refused before a row exists. The development binary composes
//! a scripted one, so the same request reaches durable state and settles. Both
//! are asserted here, in one place, because the interesting claim is the
//! difference between them rather than either on its own.

use slingshot_daemon::operation_submission::{
    ExecuteRequest, ServedTarget, SubmissionOutcome, SubmissionRefusal, settle, submit,
};
use slingshot_daemon::unavailable_operation_executor::UnavailableOperationExecutor;
use slingshot_development::slingshot_test_daemon::{DroppedProgress, TestDaemonComposition};
use slingshot_domain::command_fingerprint::{CommandFingerprint, FingerprintInput};
use slingshot_domain::installation::InstallationIdentifier;
use slingshot_domain::operation::OperationLifecycleState;
use slingshot_domain::operation_executor::{ExecutionIdentity, OperationExecutorOutcome};
use slingshot_storage::database::{OperationDatabase, RequiredSettings};
use slingshot_storage::operation_repository::{AdmissionRequest, OperationRepository};
use slingshot_test_support::fake_operation_executor::{Script, ScriptedStep};

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

/// The environment revision this daemon serves.
const REVISION: &str = "revision-1";

/// Returns one repository over a database held in memory.
fn repository() -> OperationRepository {
    OperationRepository::new(
        OperationDatabase::open_in_memory(RequiredSettings {
            page_bytes: PAGE_BYTES,
            database_pages: DATABASE_PAGES,
            busy_timeout_milliseconds: BUSY_TIMEOUT,
        })
        .expect("a database"),
    )
}

/// Returns what a daemon serves, running or not.
fn served(execution_available: bool) -> ServedTarget {
    ServedTarget {
        author_target_identity_digest: "1d".repeat(DIGEST_PAIRS),
        daemon_runtime_contract_digest: "c".repeat(DIGEST_CHARACTERS),
        execution_available,
        selected_environment_revision: REVISION.to_owned(),
    }
}

/// Returns one execute request.
fn request(identifier: &str) -> ExecuteRequest {
    let digest = served(true).author_target_identity_digest;
    let canonical = format!("{{\"paths\":[\"/{identifier}\"]}}");
    ExecuteRequest {
        admission: AdmissionRequest {
            author_target_identity: format!("opaque-identity-behind-{digest}"),
            author_target_identity_digest: digest.clone(),
            caller_identity: Some("caller-1".to_owned()),
            canonical_command: canonical.clone(),
            command_fingerprint: CommandFingerprint::derive(&FingerprintInput {
                author_target_identity_digest: digest,
                canonical_command: canonical,
                command_wire_name: "query_paths".to_owned(),
                command_semantic_contract_version: "1".to_owned(),
                selected_environment_revision: REVISION.to_owned(),
            })
            .expect("a derivable fingerprint"),
            command_wire_name: "query_paths".to_owned(),
            daemon_runtime_contract_digest: "c".repeat(DIGEST_CHARACTERS),
            installation_identifier: InstallationIdentifier::parse(&"a1".repeat(DIGEST_PAIRS))
                .expect("a legal identifier"),
            operation_identifier: identifier.to_owned(),
            selected_environment_revision: REVISION.to_owned(),
            workflow_correlation_identifier: None,
        },
        command_semantic_contract_version: "1".to_owned(),
        expected_daemon_runtime_contract_digest: "c".repeat(DIGEST_CHARACTERS),
    }
}

#[test]
fn the_product_composition_refuses_every_execute_and_creates_no_row() {
    let repository = repository();
    let digest = served(true).author_target_identity_digest;
    for identifier in ["operation-1", "operation-2"] {
        let outcome = submit(&served(false), &repository, &request(identifier), NOW)
            .expect("a classification");
        assert_eq!(
            outcome,
            SubmissionOutcome::Refused(SubmissionRefusal::ExecutionUnavailable),
            "the product build runs nothing, so it admits nothing"
        );
        assert!(repository.read(&digest, identifier).expect("a read").is_none());
    }
    assert!(
        repository.reconstruct(&digest).expect("a reconstruction").is_empty(),
        "and there is nothing for a client to find afterwards"
    );
    assert!(
        !UnavailableOperationExecutor::outcome().publishes_a_result(),
        "which is consistent with the executor it composes"
    );
}

#[test]
fn the_helper_composition_admits_once_and_invokes_the_executor_once_per_attempt() {
    let repository = repository();
    let composition = TestDaemonComposition::new(UnavailableOperationExecutor::outcome());
    let digest = served(true).author_target_identity_digest;
    composition.executor().script(
        &digest,
        "operation-1",
        Script {
            outcome: OperationExecutorOutcome::Succeeded {
                artifacts: Vec::new(),
                inline_result: Some("{}".to_owned()),
            },
            steps: vec![ScriptedStep::Progress { detail: "submitting".to_owned() }],
        },
    );

    let SubmissionOutcome::Admitted(admitted) =
        submit(&served(true), &repository, &request("operation-1"), NOW).expect("a submission")
    else {
        panic!("the helper composition admits");
    };
    let identity = ExecutionIdentity {
        attempt: 1,
        author_target_identity_digest: digest.clone(),
        operation_identifier: "operation-1".to_owned(),
    };
    let produced = composition.execute(&identity, &DroppedProgress);
    assert!(produced.publishes_a_result(), "the scripted execution succeeded");

    let replayed =
        submit(&served(true), &repository, &request("operation-1"), NOW + 1).expect("a repeat");
    assert!(
        matches!(replayed, SubmissionOutcome::Replayed(_)),
        "a repeated request finds the row rather than making a second"
    );
    let counted = composition.invocations(&digest, "operation-1");
    assert_eq!(counted.executed, 1, "and the executor ran once");

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
    assert_eq!(running.record.lifecycle_state, OperationLifecycleState::Submitting);
}

#[test]
fn the_same_identifier_in_another_partition_is_another_operation_entirely() {
    let repository = repository();
    let composition = TestDaemonComposition::new(UnavailableOperationExecutor::outcome());
    let here = served(true).author_target_identity_digest;
    let elsewhere = "2d".repeat(DIGEST_PAIRS);

    submit(&served(true), &repository, &request("operation-1"), NOW).expect("a submission");
    let mut over_there = request("operation-1");
    over_there.admission.author_target_identity_digest = elsewhere.clone();
    over_there.admission.author_target_identity = format!("opaque-identity-behind-{elsewhere}");
    let refused = submit(&served(true), &repository, &over_there, NOW).expect("a classification");
    assert_eq!(
        refused,
        SubmissionOutcome::Refused(SubmissionRefusal::TargetMismatch),
        "this daemon serves one partition, so it refuses the other rather than admitting it"
    );

    for target in [&here, &elsewhere] {
        let identity = ExecutionIdentity {
            attempt: 1,
            author_target_identity_digest: target.clone(),
            operation_identifier: "operation-1".to_owned(),
        };
        composition.execute(&identity, &DroppedProgress);
    }
    assert_eq!(
        composition.invocations(&here, "operation-1").replayed,
        1,
        "the executor counts each partition's work separately"
    );
    assert_eq!(composition.invocations(&elsewhere, "operation-1").replayed, 1);
}

#[test]
fn settling_through_the_helper_records_exactly_what_the_script_produced() {
    let repository = repository();
    let composition = TestDaemonComposition::new(UnavailableOperationExecutor::outcome());
    let digest = served(true).author_target_identity_digest;
    composition.executor().script(
        &digest,
        "operation-1",
        Script {
            outcome: OperationExecutorOutcome::Succeeded {
                artifacts: Vec::new(),
                inline_result: Some("{}".to_owned()),
            },
            steps: Vec::new(),
        },
    );
    let SubmissionOutcome::Admitted(admitted) =
        submit(&served(true), &repository, &request("operation-1"), NOW).expect("a submission")
    else {
        panic!("the helper composition admits");
    };

    let identity = ExecutionIdentity {
        attempt: 1,
        author_target_identity_digest: digest.clone(),
        operation_identifier: "operation-1".to_owned(),
    };
    let produced = composition.execute(&identity, &DroppedProgress);
    let mut current = *admitted;
    for reached in [
        OperationLifecycleState::Submitting,
        OperationLifecycleState::Accepted,
        OperationLifecycleState::Running,
    ] {
        current = repository
            .apply(
                &digest,
                "operation-1",
                current.record.revision,
                &slingshot_domain::operation::OperationFact::Lifecycle { lifecycle_state: reached },
                NOW,
            )
            .expect("an advance");
    }
    let settled = settle(&repository, &current, &produced, NOW).expect("a settlement");
    assert_eq!(settled.record.lifecycle_state, OperationLifecycleState::Succeeded);
    assert!(settled.result_disposition.is_some(), "and the row says where the result went");
}
