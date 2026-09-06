//! Pending admission counts and insertion share one immediate transaction.

use slingshot_domain::command_fingerprint::{CommandFingerprint, FingerprintInput};
use slingshot_domain::daemon_runtime_contract::{DIGEST_OCTETS, DaemonRuntimeContract};
use slingshot_domain::installation::InstallationIdentifier;
use slingshot_storage::database::{OperationDatabase, RequiredSettings};
use slingshot_storage::operation_repository::{
    AdmissionOutcome, AdmissionRequest, OperationRepository, PendingAdmissionCapacity,
    RepositoryFailure,
};
use std::collections::BTreeSet;
use std::path::Path;

fn open(path: &Path) -> OperationRepository {
    let limits = DaemonRuntimeContract::embedded();
    OperationRepository::new(
        OperationDatabase::open(
            path,
            RequiredSettings {
                page_bytes: limits.limit("sqlite_page_bytes"),
                database_pages: limits.limit("maximum_sqlite_database_pages"),
                busy_timeout_milliseconds: limits.limit("database_busy_timeout_milliseconds"),
            },
        )
        .unwrap(),
    )
}

fn request(identifier: &str, caller: Option<&str>) -> AdmissionRequest {
    let target = "a".repeat(DIGEST_OCTETS * 2);
    let revision = "b".repeat(DIGEST_OCTETS * 2);
    let command = r#"{"root_path":"/content/example"}"#.to_owned();
    AdmissionRequest {
        author_target_identity: target.clone(),
        author_target_identity_digest: target.clone(),
        caller_identity: caller.map(str::to_owned),
        canonical_command: command.clone(),
        command_fingerprint: CommandFingerprint::derive(&FingerprintInput {
            author_target_identity_digest: target,
            canonical_command: command,
            command_wire_name: "query_paths".to_owned(),
            command_semantic_contract_version: "1.0.0".to_owned(),
            selected_environment_revision: revision.clone(),
        })
        .unwrap(),
        command_wire_name: "query_paths".to_owned(),
        daemon_runtime_contract_digest: DaemonRuntimeContract::embedded_digest()
            .as_text()
            .to_owned(),
        installation_identifier: InstallationIdentifier::parse(&"d".repeat(DIGEST_OCTETS * 2))
            .unwrap(),
        operation_identifier: identifier.to_owned(),
        selected_environment_revision: revision,
        workflow_correlation_identifier: None,
    }
}

#[test]
fn replay_and_conflict_survive_full_pending_capacity() {
    let root = tempfile::tempdir().unwrap();
    let repository = open(&root.path().join("operations.sqlite"));
    let active = BTreeSet::new();
    let capacity = PendingAdmissionCapacity {
        active_operations: &active,
        global_pending: 1,
        pending_per_caller: 1,
    };
    let first = request("first", None);
    assert!(matches!(
        repository.admit_with_pending_capacity(&first, 1, capacity).unwrap(),
        AdmissionOutcome::Admitted(_)
    ));
    assert!(matches!(
        repository.admit_with_pending_capacity(&first, 2, capacity).unwrap(),
        AdmissionOutcome::Replayed(_)
    ));
    let mut changed = first.clone();
    changed.selected_environment_revision = "c".repeat(DIGEST_OCTETS * 2);
    assert!(matches!(
        repository.admit_with_pending_capacity(&changed, 3, capacity).unwrap(),
        AdmissionOutcome::Conflict(_)
    ));
    assert!(matches!(
        repository.admit_with_pending_capacity(&request("second", None), 4, capacity),
        Err(RepositoryFailure::PendingCapacity { per_caller: false, held: 1, limit: 1 })
    ));
    assert!(repository.read(&first.author_target_identity_digest, "second").unwrap().is_none());
    repository
        .settle_success(
            &first.author_target_identity_digest,
            "first",
            &slingshot_domain::operation::SuccessfulSettlement {
                artifacts: Vec::new(),
                inline_result: Some("{}".to_owned()),
                expected_lifecycle_state:
                    slingshot_domain::operation::OperationLifecycleState::Queued,
                expected_revision: 1,
                settled_at_unix_milliseconds: 5,
            },
        )
        .unwrap();
    assert!(matches!(
        repository.admit_with_pending_capacity(&request("second", None), 6, capacity).unwrap(),
        AdmissionOutcome::Admitted(_)
    ));
    assert!(matches!(
        repository.admit_with_pending_capacity(&first, 7, capacity).unwrap(),
        AdmissionOutcome::Replayed(_)
    ));
}

#[test]
fn caller_capacity_and_live_slots_are_counted_separately() {
    let root = tempfile::tempdir().unwrap();
    let repository = open(&root.path().join("operations.sqlite"));
    let active = BTreeSet::new();
    let capacity = PendingAdmissionCapacity {
        active_operations: &active,
        global_pending: 3,
        pending_per_caller: 1,
    };
    repository
        .admit_with_pending_capacity(&request("first", Some("caller-a")), 1, capacity)
        .unwrap();
    assert!(matches!(
        repository.admit_with_pending_capacity(&request("second", Some("caller-a")), 2, capacity),
        Err(RepositoryFailure::PendingCapacity { per_caller: true, held: 1, limit: 1 })
    ));
    repository
        .admit_with_pending_capacity(&request("third", Some("caller-b")), 3, capacity)
        .unwrap();
    let active = BTreeSet::from(["first".to_owned()]);
    let capacity = PendingAdmissionCapacity { active_operations: &active, ..capacity };
    assert!(matches!(
        repository
            .admit_with_pending_capacity(&request("second", Some("caller-a")), 4, capacity)
            .unwrap(),
        AdmissionOutcome::Admitted(_)
    ));
}

#[test]
fn concurrent_connections_cannot_both_take_the_last_pending_slot() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("operations.sqlite");
    drop(open(&path));
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let workers: Vec<_> = ["first", "second"]
        .into_iter()
        .map(|identifier| {
            let path = path.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let repository = open(&path);
                let active = BTreeSet::new();
                barrier.wait();
                repository.admit_with_pending_capacity(
                    &request(identifier, None),
                    1,
                    PendingAdmissionCapacity {
                        active_operations: &active,
                        global_pending: 1,
                        pending_per_caller: 1,
                    },
                )
            })
        })
        .collect();
    let results: Vec<_> = workers.into_iter().map(|worker| worker.join().unwrap()).collect();
    assert_eq!(
        results.iter().filter(|result| matches!(result, Ok(AdmissionOutcome::Admitted(_)))).count(),
        1
    );
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(result, Err(RepositoryFailure::PendingCapacity { .. })))
            .count(),
        1
    );
    assert_eq!(
        open(&path).reconstruct(&request("", None).author_target_identity_digest).unwrap().len(),
        1
    );
}
