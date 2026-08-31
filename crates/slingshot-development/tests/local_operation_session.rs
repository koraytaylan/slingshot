//! Two whole sessions, replayed against the real services and byte-matched.
//!
//! Each committed session is a sequence of steps and the exact answer each one
//! produces. The test drives the real services and re-renders what happened,
//! then compares the bytes. So a change anywhere in the chain - admission,
//! lifecycle, recovery, resume, query, listing, maintenance - shows up here as
//! a differing line rather than as a test that quietly still passes.
//!
//! The two sessions are the two compositions. The product one ends almost
//! immediately, because a build that composes no executor admits nothing; the
//! helper one runs a whole operation through the case worth caring about, where
//! the remote succeeds and the result cannot be stored.

use serde_json::{Value, json};
use slingshot_daemon::operation_maintenance::{PreviewRequest, apply, preview};
use slingshot_daemon::operation_queries::{
    NEWEST_FIRST, OperationResult, PageCursor, list, result, status,
};
use slingshot_daemon::operation_recovery::{ResumeRequest, ResumeResponse, resume};
use slingshot_daemon::operation_submission::{
    ExecuteRequest, ServedTarget, SubmissionOutcome, SubmissionRefusal, capacity_unavailable,
    submit,
};
use slingshot_daemon::request_dispatch::{Dispatch, DispatchPolicy, RequestKind};
use slingshot_domain::command_fingerprint::{CommandFingerprint, FingerprintInput};
use slingshot_domain::installation::InstallationIdentifier;
use slingshot_domain::operation::{OperationFact, OperationLifecycleState, RecoveryCategory};
use slingshot_storage::database::{OperationDatabase, RequiredSettings};
use slingshot_storage::maintenance::ApplyOutcome;
use slingshot_storage::operation_repository::{
    AdmissionRequest, OperationRepository, ResultDisposition,
};

/// The product session this test replays.
const PRODUCT: &str = include_str!("fixtures/local-operation-session/product.jsonl");

/// The helper session this test replays.
const HELPER: &str = include_str!("fixtures/local-operation-session/helper.jsonl");

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

/// One instant, for a session that does not care which.
const NOW: u64 = 1_700_000_000_000;

/// A cutoff past every settlement in these sessions.
const EVERYTHING: u64 = u64::MAX;

/// Rows one listing page takes.
const PAGE_SIZE: u64 = 16;

/// The operation protocol version this daemon serves.
const SERVED_VERSION: u64 = 1;

/// The environment revision these sessions run at.
const REVISION: &str = "revision-1";

/// The operation both sessions ask for.
const OPERATION: &str = "operation-1";

/// A version no daemon in these sessions serves.
const UNSERVED_VERSION: u64 = 9;

/// Attempts the session's recovery has made.
const FIRST_ATTEMPT: u32 = 1;

/// Returns the settings every connection is held to.
fn settings() -> RequiredSettings {
    RequiredSettings {
        page_bytes: PAGE_BYTES,
        database_pages: DATABASE_PAGES,
        busy_timeout_milliseconds: BUSY_TIMEOUT,
    }
}

/// Returns the digest this session's author target has.
fn partition() -> String {
    "1d".repeat(DIGEST_PAIRS)
}

/// Returns what the daemon serves, running or not.
fn served(execution_available: bool) -> ServedTarget {
    ServedTarget {
        author_target_identity_digest: partition(),
        daemon_runtime_contract_digest: "c".repeat(DIGEST_CHARACTERS),
        execution_available,
        selected_environment_revision: REVISION.to_owned(),
    }
}

/// Returns the session's execute request.
fn execute_request() -> ExecuteRequest {
    let digest = partition();
    let canonical = "{\"paths\":[\"/content\"]}";
    ExecuteRequest {
        admission: AdmissionRequest {
            author_target_identity: format!("opaque-identity-behind-{digest}"),
            author_target_identity_digest: digest.clone(),
            caller_identity: Some("caller-1".to_owned()),
            canonical_command: canonical.to_owned(),
            command_fingerprint: CommandFingerprint::derive(&FingerprintInput {
                author_target_identity_digest: digest,
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
        },
        command_semantic_contract_version: "1".to_owned(),
        expected_daemon_runtime_contract_digest: "c".repeat(DIGEST_CHARACTERS),
    }
}

/// Returns one repository over a database held in memory.
fn repository() -> OperationRepository {
    OperationRepository::new(OperationDatabase::open_in_memory(settings()).expect("a database"))
}

/// Returns the session's name for one refusal.
///
/// Written out rather than derived from a formatter, so the committed bytes
/// compare against a spelling this suite chose and a rename of the variant
/// shows up as a differing line rather than silently changing the fixture.
fn refusal_name(refusal: &SubmissionRefusal) -> &'static str {
    match refusal {
        SubmissionRefusal::ExecutionUnavailable => "execution_unavailable",
        SubmissionRefusal::TargetMismatch => "target_mismatch",
        SubmissionRefusal::RevisionMismatch => "revision_mismatch",
        SubmissionRefusal::ContractMismatch => "contract_mismatch",
    }
}

/// Renders one session's steps as the committed fixture spells them.
fn render(steps: &[Value]) -> String {
    steps
        .iter()
        .map(|step| serde_json::to_string(step).expect("a step renders"))
        .collect::<Vec<String>>()
        .join("\n")
        + "\n"
}

/// Returns the committed steps of one session.
fn committed(session: &str) -> Vec<Value> {
    session
        .lines()
        .map(|line| serde_json::from_str(line).expect("every fixture line is one object"))
        .collect()
}

/// Returns one step's note, so the replay carries the fixture's own words.
fn note(committed: &[Value], index: usize) -> String {
    committed[index]["note"].as_str().expect("a note").to_owned()
}

#[test]
fn the_product_session_matches_the_committed_bytes() {
    let expected = committed(PRODUCT);
    let repository = repository();
    let policy =
        DispatchPolicy { stopping: false, supported_operation_versions: vec![SERVED_VERSION] };
    let mut produced = Vec::new();

    let incompatible = policy.dispatch(RequestKind::RetainedControl, UNSERVED_VERSION);
    produced.push(json!({
        "kind": "retained_control",
        "note": note(&expected, produced.len()),
        "operation_version": UNSERVED_VERSION,
        "outcome": if matches!(incompatible, Dispatch::Serve(_)) { "serve" } else { "refuse" },
        "step": "dispatch",
    }));

    let compatible = policy.dispatch(RequestKind::Execute, SERVED_VERSION);
    produced.push(json!({
        "kind": "execute",
        "note": note(&expected, produced.len()),
        "operation_version": SERVED_VERSION,
        "outcome": if matches!(compatible, Dispatch::Serve(_)) { "serve" } else { "refuse" },
        "step": "dispatch",
    }));

    let refused =
        submit(&served(false), &repository, &execute_request(), NOW).expect("a classification");
    let SubmissionOutcome::Refused(refusal) = refused else {
        panic!("the product build refuses every execute");
    };
    produced.push(json!({
        "note": note(&expected, produced.len()),
        "outcome": "refused",
        "refusal": refusal_name(&refusal),
        "step": "execute",
    }));

    let found = repository.read(&partition(), OPERATION).expect("a read");
    produced.push(json!({
        "note": note(&expected, produced.len()),
        "operation": OPERATION,
        "outcome": if found.is_none() { "absent" } else { "present" },
        "step": "read",
    }));

    let page = list(
        &repository,
        &partition(),
        PageCursor { before_enqueue_sequence: NEWEST_FIRST },
        PAGE_SIZE,
    )
    .expect("a page");
    produced.push(json!({
        "note": note(&expected, produced.len()),
        "rows": page.rows.len(),
        "step": "list",
    }));

    assert_eq!(render(&produced), PRODUCT, "the product session byte-matches what was committed");
}

#[test]
fn the_helper_session_matches_the_committed_bytes() {
    let expected = committed(HELPER);
    let repository = repository();
    let digest = partition();
    let mut produced = Vec::new();
    let mut current = None;

    for outcome in ["admitted", "replayed"] {
        let submitted =
            submit(&served(true), &repository, &execute_request(), NOW).expect("a submission");
        let summary = match &submitted {
            SubmissionOutcome::Admitted(summary)
            | SubmissionOutcome::Replayed(summary)
            | SubmissionOutcome::Conflict(summary) => (**summary).clone(),
            SubmissionOutcome::Refused(refusal) => panic!("the helper session admits: {refusal}"),
        };
        produced.push(json!({
            "lifecycle_state": "queued",
            "note": note(&expected, produced.len()),
            "outcome": outcome,
            "revision": summary.record.revision,
            "step": "execute",
        }));
        current = Some(summary);
    }
    let mut summary = current.expect("the session admitted something");

    for reached in [
        OperationLifecycleState::Submitting,
        OperationLifecycleState::Accepted,
        OperationLifecycleState::Running,
    ] {
        summary = repository
            .apply(
                &digest,
                OPERATION,
                summary.record.revision,
                &OperationFact::Lifecycle { lifecycle_state: reached },
                NOW,
            )
            .expect("a legal advance");
        produced.push(json!({
            "note": note(&expected, produced.len()),
            "revision": summary.record.revision,
            "step": "advance",
            "to": format!("{reached:?}").to_lowercase(),
        }));
    }

    summary = repository
        .apply(
            &digest,
            OPERATION,
            summary.record.revision,
            &OperationFact::Recovery {
                recovery: capacity_unavailable("the result does not fit", FIRST_ATTEMPT, NOW),
            },
            NOW,
        )
        .expect("a recovery fact");
    produced.push(json!({
        "category": "persistent_capacity_unavailable",
        "evidence": "authoritative_remote_success",
        "note": note(&expected, produced.len()),
        "revision": summary.record.revision,
        "step": "recovery",
    }));

    let held = status(&repository, &digest, OPERATION).expect("a status");
    produced.push(json!({
        "lifecycle_state": format!("{:?}", held.lifecycle_state).to_lowercase(),
        "note": note(&expected, produced.len()),
        "revision": held.revision,
        "step": "status",
        "waiting": held.outstanding_recovery.is_some(),
    }));

    let produced_result = result(&repository, &digest, OPERATION).expect("a result");
    let OperationResult::RecoveryRequired { recovery } = &produced_result else {
        panic!("the remote succeeded and the result does not fit, which is not an ending");
    };
    produced.push(json!({
        "evidence": "authoritative_remote_success",
        "note": note(&expected, produced.len()),
        "outcome": "recovery_required",
        "step": "result",
    }));
    assert_eq!(recovery.category, RecoveryCategory::PersistentCapacityUnavailable);

    let asked = ResumeRequest {
        author_target_identity_digest: digest.clone(),
        expected_recovery_category: RecoveryCategory::PersistentCapacityUnavailable,
        expected_revision: summary.record.revision,
        operation_identifier: OPERATION.to_owned(),
        selected_environment_revision: REVISION.to_owned(),
    };
    for outcome in ["applied", "replayed"] {
        let resumed = resume(&repository, &asked, NOW).expect("a resume");
        let seen = match resumed {
            ResumeResponse::Applied(_) => "applied",
            ResumeResponse::Replayed(_) => "replayed",
            ResumeResponse::Refused(refusal) => panic!("the session resumes: {refusal}"),
        };
        assert_eq!(seen, outcome, "the session's resume outcomes are in this order");
        produced.push(json!({
            "note": note(&expected, produced.len()),
            "outcome": seen,
            "step": "resume",
        }));
    }

    summary = repository
        .apply(
            &digest,
            OPERATION,
            summary.record.revision,
            &OperationFact::Lifecycle { lifecycle_state: OperationLifecycleState::Succeeded },
            NOW,
        )
        .expect("the local half finishes");
    produced.push(json!({
        "note": note(&expected, produced.len()),
        "revision": summary.record.revision,
        "step": "advance",
        "to": "succeeded",
    }));
    repository
        .record_result_disposition(
            &digest,
            OPERATION,
            summary.record.revision,
            ResultDisposition::Inline,
        )
        .expect("a disposition");
    produced.push(json!({
        "disposition": "inline",
        "note": note(&expected, produced.len()),
        "outcome": "succeeded",
        "step": "result",
    }));

    let page =
        list(&repository, &digest, PageCursor { before_enqueue_sequence: NEWEST_FIRST }, PAGE_SIZE)
            .expect("a page");
    produced.push(json!({
        "note": note(&expected, produced.len()),
        "rows": page.rows.len(),
        "step": "list",
    }));

    let reviewed = preview(
        repository.database(),
        &PreviewRequest {
            author_target_identity_digest: digest.clone(),
            before_unix_milliseconds: EVERYTHING,
            limit: PAGE_SIZE,
        },
    )
    .expect("a preview");
    produced.push(json!({
        "note": note(&expected, produced.len()),
        "removals": reviewed.manifest.removals.len(),
        "step": "maintenance_preview",
    }));

    let applied = apply(repository.database(), &reviewed, NOW).expect("an apply");
    let ApplyOutcome::Applied(receipt) = applied else { panic!("a fresh manifest applies") };
    produced.push(json!({
        "note": note(&expected, produced.len()),
        "outcome": "applied",
        "released": receipt.released_operation_rows,
        "step": "maintenance_apply",
    }));

    let page =
        list(&repository, &digest, PageCursor { before_enqueue_sequence: NEWEST_FIRST }, PAGE_SIZE)
            .expect("a page");
    produced.push(json!({
        "note": note(&expected, produced.len()),
        "rows": page.rows.len(),
        "step": "list",
    }));

    assert_eq!(render(&produced), HELPER, "the helper session byte-matches what was committed");
}
