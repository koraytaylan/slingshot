//! Several clients contending at one moment, on purpose.
//!
//! A concurrency test that starts its clients in a loop is not testing
//! concurrency: the first usually finishes before the last starts. Everything
//! here goes through a barrier so the interleavings actually happen, and every
//! assertion is about what the daemon guarantees under contention rather than
//! about what it happens to do when nobody else is asking.

use std::sync::Arc;
use std::time::Duration;

use slingshot_daemon::local_server::ConnectionCapacity;
use slingshot_domain::command_fingerprint::{CommandFingerprint, FingerprintInput};
use slingshot_domain::installation::InstallationIdentifier;
use slingshot_local_protocol::foundation_contract::FoundationContract;
use slingshot_storage::database::{OperationDatabase, RequiredSettings};
use slingshot_storage::operation_repository::{
    AdmissionOutcome, AdmissionRequest, OperationRepository,
};
use slingshot_test_support::operation_fault_injection::FaultInjector;
use slingshot_test_support::process_barrier::{Arrival, ProcessBarrier};

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

/// Clients one cohort runs, from the foundation contract's walking start.
const COHORT: usize = 20;

/// Milliseconds a barrier waits before it reports nobody came.
const BARRIER_DEADLINE_MILLISECONDS: u64 = 30_000;

/// One instant, for a test that does not care which.
const NOW: u64 = 1_700_000_000_000;

/// The environment revision these fixtures are admitted under.
const REVISION: &str = "revision-1";

/// The operation every contender asks for.
const OPERATION: &str = "operation-1";

/// Returns the settings every connection is held to.
fn settings() -> RequiredSettings {
    RequiredSettings {
        page_bytes: PAGE_BYTES,
        database_pages: DATABASE_PAGES,
        busy_timeout_milliseconds: BUSY_TIMEOUT,
    }
}

/// Returns the digest one principal's author target has.
fn partition(principal: &str) -> String {
    principal.repeat(DIGEST_PAIRS)
}

/// Returns one admission request against `digest`.
fn admission(digest: &str) -> AdmissionRequest {
    let canonical = "{\"paths\":[\"/content\"]}";
    AdmissionRequest {
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
    }
}

/// Returns how long a barrier waits before it gives up.
fn barrier_deadline() -> Duration {
    Duration::from_millis(BARRIER_DEADLINE_MILLISECONDS)
}

#[test]
fn a_barrier_releases_only_once_everyone_has_arrived() {
    let barrier = ProcessBarrier::expecting(COHORT);
    let arrivals: Vec<Arrival> = std::thread::scope(|scope| {
        let running: Vec<std::thread::ScopedJoinHandle<'_, Arrival>> = (0..COHORT)
            .map(|_| {
                let barrier = barrier.clone();
                scope.spawn(move || barrier.arrive(barrier_deadline()))
            })
            .collect();
        running.into_iter().map(|handle| handle.join().expect("a participant finishes")).collect()
    });

    assert_eq!(arrivals.len(), COHORT, "every participant answered");
    assert!(
        arrivals.iter().all(|arrival| *arrival == Arrival::Released),
        "and all of them were released, because all of them arrived"
    );
    assert_eq!(barrier.arrived(), COHORT);
}

#[test]
fn a_barrier_nobody_completes_reports_it_rather_than_hanging() {
    let barrier = ProcessBarrier::expecting(COHORT);
    let arrival = barrier.arrive(Duration::from_millis(1));
    assert_eq!(
        arrival,
        Arrival::DeadlineElapsed,
        "one stuck participant must not turn a suite into one that never finishes"
    );
}

#[test]
fn a_cohort_arriving_at_once_creates_one_operation_and_every_client_gets_it() {
    let directory = tempfile::tempdir().expect("a directory");
    let path = Arc::new(directory.path().join("operations.sqlite3"));
    let digest = partition("1d");
    // Created and dropped, so the schema exists before the cohort contends
    // over it: migrating under contention is a different test.
    drop(OperationRepository::new(OperationDatabase::open(&path, settings()).expect("a database")));

    let barrier = ProcessBarrier::expecting(COHORT);
    let admitted: Vec<bool> = std::thread::scope(|scope| {
        let running: Vec<std::thread::ScopedJoinHandle<'_, bool>> = (0..COHORT)
            .map(|_| {
                let barrier = barrier.clone();
                let path = Arc::clone(&path);
                let digest = digest.clone();
                scope.spawn(move || {
                    let repository = OperationRepository::new(
                        OperationDatabase::open(&path, settings()).expect("a database"),
                    );
                    assert_eq!(barrier.arrive(barrier_deadline()), Arrival::Released);
                    let outcome =
                        repository.admit(&admission(&digest), NOW).expect("a classification");
                    matches!(outcome, AdmissionOutcome::Admitted(_))
                })
            })
            .collect();
        running.into_iter().map(|handle| handle.join().expect("a client finishes")).collect()
    });

    assert_eq!(
        admitted.iter().filter(|won| **won).count(),
        1,
        "exactly one of twenty simultaneous clients created the operation"
    );
    let repository =
        OperationRepository::new(OperationDatabase::open(&path, settings()).expect("a database"));
    assert_eq!(
        repository.reconstruct(&digest).expect("a reconstruction").len(),
        1,
        "and there is one row, not twenty"
    );
}

#[test]
fn a_cohort_against_distinct_targets_does_not_contend_at_all() {
    let directory = tempfile::tempdir().expect("a directory");
    let path = Arc::new(directory.path().join("operations.sqlite3"));
    // Created and dropped, so the schema exists before the cohort contends
    // over it: migrating under contention is a different test.
    drop(OperationRepository::new(OperationDatabase::open(&path, settings()).expect("a database")));

    let barrier = ProcessBarrier::expecting(COHORT);
    let admitted: Vec<bool> = std::thread::scope(|scope| {
        let running: Vec<std::thread::ScopedJoinHandle<'_, bool>> = (0..COHORT)
            .map(|index| {
                let barrier = barrier.clone();
                let path = Arc::clone(&path);
                scope.spawn(move || {
                    let repository = OperationRepository::new(
                        OperationDatabase::open(&path, settings()).expect("a database"),
                    );
                    let digest = format!("{index:02}").repeat(DIGEST_PAIRS);
                    assert_eq!(barrier.arrive(barrier_deadline()), Arrival::Released);
                    let outcome =
                        repository.admit(&admission(&digest), NOW).expect("a classification");
                    matches!(outcome, AdmissionOutcome::Admitted(_))
                })
            })
            .collect();
        running.into_iter().map(|handle| handle.join().expect("a client finishes")).collect()
    });

    assert_eq!(
        admitted.iter().filter(|won| **won).count(),
        COHORT,
        "the same identifier against twenty targets is twenty operations, and all of them admit"
    );
}

#[test]
fn connection_capacity_is_released_by_a_cohort_that_leaves() {
    let contract = FoundationContract::embedded();
    let capacity = Arc::new(ConnectionCapacity::declared(&contract));
    let barrier = ProcessBarrier::expecting(COHORT);

    std::thread::scope(|scope| {
        for _ in 0..COHORT {
            let barrier = barrier.clone();
            let capacity = Arc::clone(&capacity);
            scope.spawn(move || {
                let permit = capacity.claim().expect("a connection below the bound");
                assert_eq!(barrier.arrive(barrier_deadline()), Arrival::Released);
                drop(permit);
            });
        }
    });

    assert_eq!(
        capacity.in_use(),
        0,
        "a cohort that all connected and all left holds nothing afterwards"
    );
    capacity.claim().expect("so a later client still fits");
}

#[test]
fn a_fault_injector_is_safe_to_share_between_contending_clients() {
    let injector = Arc::new(FaultInjector::passive());
    let barrier = ProcessBarrier::expecting(COHORT);
    injector
        .arm(slingshot_test_support::operation_fault_injection::Checkpoint::AfterAdmissionCommit);

    let interrupted: Vec<bool> = std::thread::scope(|scope| {
        let running: Vec<std::thread::ScopedJoinHandle<'_, bool>> = (0..COHORT)
            .map(|_| {
                let barrier = barrier.clone();
                let injector = Arc::clone(&injector);
                scope.spawn(move || {
                    assert_eq!(barrier.arrive(barrier_deadline()), Arrival::Released);
                    injector.reach(
                        slingshot_test_support::operation_fault_injection::Checkpoint::AfterAdmissionCommit,
                    ) == slingshot_test_support::operation_fault_injection::Instruction::Interrupt
                })
            })
            .collect();
        running.into_iter().map(|handle| handle.join().expect("a client finishes")).collect()
    });

    assert_eq!(
        interrupted.iter().filter(|stopped| **stopped).count(),
        1,
        "an armed checkpoint interrupts exactly one of twenty contenders, not all of them"
    );
}
