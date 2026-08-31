//! Admitting an operation, and what a repeated identifier means.
//!
//! A caller picks its own identifier, so the same one legitimately names
//! different work against different targets and the same work after a lost
//! acknowledgement. These prove which is which, and that neither answer is
//! reached by writing anything the caller did not ask for.

use slingshot_domain::operation::OperationLifecycleState;
use slingshot_storage::database::OperationDatabase;
use slingshot_storage::operation_repository::AdmissionOutcome;

use crate::fixtures::*;

#[test]
fn one_identifier_names_one_row_and_a_repeat_returns_that_row() {
    let store = in_memory();
    let digest = partition(FIRST_PRINCIPAL);
    let asked = request(&digest, "operation-1", "{\"paths\":[\"/content\"]}", "revision-1");

    let admitted = store.admit(&asked, NOW).expect("a first admission");
    let AdmissionOutcome::Admitted(first) = &admitted else {
        panic!("the first admission admits: {admitted:?}");
    };
    assert_eq!(first.record.lifecycle_state, OperationLifecycleState::Queued);
    assert_eq!(first.record.revision, 1, "an admitted operation is at its first revision");
    assert_eq!(first.enqueue_sequence, 1, "and first in its partition's arrival order");
    assert_eq!(
        first.installation_identifier,
        installation(),
        "the admitting installation is written in the admission transaction"
    );

    let again = store.admit(&asked, SECOND_INSTANT).expect("a repeat");
    let AdmissionOutcome::Replayed(second) = &again else {
        panic!("the same work under the same identifier replays: {again:?}");
    };
    assert_eq!(second, first, "and replays the row that is there, unchanged");
}

#[test]
fn a_different_command_under_one_identifier_conflicts_without_writing() {
    let store = in_memory();
    let digest = partition(FIRST_PRINCIPAL);
    let first = request(&digest, "operation-1", "{\"paths\":[\"/content\"]}", "revision-1");
    let admitted = store.admit(&first, NOW).expect("a first admission");

    let other_command = request(&digest, "operation-1", "{\"paths\":[\"/other\"]}", "revision-1");
    let outcome = store.admit(&other_command, SECOND_INSTANT).expect("a classification");
    let AdmissionOutcome::Conflict(held) = &outcome else {
        panic!("different work under a taken identifier conflicts: {outcome:?}");
    };
    assert_eq!(held.as_ref(), admitted.summary(), "and the stored row is returned untouched");

    let other_revision =
        request(&digest, "operation-1", "{\"paths\":[\"/content\"]}", "revision-2");
    let outcome = store.admit(&other_revision, THIRD_INSTANT).expect("a classification");
    assert!(
        matches!(outcome, AdmissionOutcome::Conflict(_)),
        "and so does the same command against another selected revision: {outcome:?}"
    );
    assert_eq!(
        store.read(&digest, "operation-1").expect("a read").as_ref(),
        Some(admitted.summary()),
        "neither conflict changed a byte"
    );
    assert_eq!(
        store.reconstruct(&digest).expect("a reconstruction").len(),
        1,
        "and neither created a second row"
    );
}

#[test]
fn one_identifier_is_independent_in_every_target_partition() {
    let store = in_memory();
    let first = partition(FIRST_PRINCIPAL);
    let second = partition(SECOND_PRINCIPAL);
    let command = "{\"paths\":[\"/content\"]}";

    let one = store.admit(&request(&first, "operation-1", command, "revision-1"), NOW);
    let two = store.admit(&request(&second, "operation-1", command, "revision-1"), NOW);
    assert!(
        matches!(one.expect("a first admission"), AdmissionOutcome::Admitted(_)),
        "the first partition admits"
    );
    assert!(
        matches!(two.expect("a second admission"), AdmissionOutcome::Admitted(_)),
        "and so does the second, under the same identifier, because it is another operation"
    );

    let held = store.read(&first, "operation-1").expect("a read").expect("a row");
    let neighbour = store.read(&second, "operation-1").expect("a read").expect("a row");
    assert_ne!(
        held.command_fingerprint, neighbour.command_fingerprint,
        "the fingerprint binds the partition, so identical commands fingerprint differently"
    );
    assert_eq!(held.enqueue_sequence, 1, "each partition counts its own arrivals");
    assert_eq!(neighbour.enqueue_sequence, 1);
}

#[test]
fn a_replay_never_reaches_across_a_partition() {
    let store = in_memory();
    let first = partition(FIRST_PRINCIPAL);
    let second = partition(SECOND_PRINCIPAL);
    let command = "{\"paths\":[\"/content\"]}";
    store.admit(&request(&first, "operation-1", command, "revision-1"), NOW).expect("admitted");

    let across = store
        .admit(&request(&second, "operation-1", command, "revision-1"), NOW)
        .expect("a classification");
    assert!(
        matches!(across, AdmissionOutcome::Admitted(_)),
        "an identifier taken in another partition is not taken here: {across:?}"
    );
    assert!(
        store.read(&second, "operation-2").expect("a read").is_none(),
        "and a partition answers only for its own rows"
    );
}

#[test]
fn concurrent_identical_admission_creates_one_row_and_every_caller_gets_it() {
    let directory = tempfile::tempdir().expect("a directory");
    let path = directory.path().join("operations.sqlite3");
    let digest = partition(FIRST_PRINCIPAL);
    repository(&path);

    let raced = digest.clone();
    let admitted = race(&path, move |store, _| {
        let asked = request(&raced, OPERATION, "{\"paths\":[\"/a\"]}", "revision-1");
        matches!(store.admit(&asked, NOW).expect("a classification"), AdmissionOutcome::Admitted(_))
    });
    assert_eq!(
        admitted.iter().filter(|won| **won).count(),
        1,
        "exactly one of them admitted the row"
    );

    let store = repository(&path);
    let held = store.reconstruct(&digest).expect("a reconstruction");
    assert_eq!(held.len(), 1, "and one row exists");
    assert_eq!(held[0].record.revision, 1, "which nobody rewrote after it was written");
}

#[test]
fn concurrent_conflicting_admission_yields_one_winner_and_deterministic_conflicts() {
    let directory = tempfile::tempdir().expect("a directory");
    let path = directory.path().join("operations.sqlite3");
    let digest = partition(FIRST_PRINCIPAL);
    repository(&path);

    let raced = digest.clone();
    let admitted = race(&path, move |store, index| {
        let command = format!("{{\"paths\":[\"/{index}\"]}}");
        let asked = request(&raced, OPERATION, &command, "revision-1");
        matches!(store.admit(&asked, NOW).expect("a classification"), AdmissionOutcome::Admitted(_))
    });
    assert_eq!(admitted.iter().filter(|won| **won).count(), 1, "one writer wins");
    assert_eq!(
        admitted.iter().filter(|won| !**won).count(),
        CONTENDERS - 1,
        "and every other is told it conflicts rather than overwriting"
    );

    let store = repository(&path);
    let held = store.reconstruct(&digest).expect("a reconstruction");
    assert_eq!(held.len(), 1, "one row exists");
    assert_eq!(held[0].record.revision, 1, "which nobody rewrote after it was written");
}

#[test]
fn an_admitted_row_always_carries_the_installation_that_admitted_it() {
    let directory = tempfile::tempdir().expect("a directory");
    let path = directory.path().join("operations.sqlite3");
    let digest = partition(FIRST_PRINCIPAL);
    let command = "{\"paths\":[\"/content\"]}";
    {
        let store = repository(&path);
        store
            .admit(&request(&digest, "operation-1", command, "revision-1"), NOW)
            .expect("admitted");
    }

    let store = repository(&path);
    let reopened = store.read(&digest, "operation-1").expect("a read").expect("a row");
    assert_eq!(
        reopened.installation_identifier,
        installation(),
        "the snapshot survives the process that took it"
    );

    let mut later = request(&digest, "operation-1", command, "revision-1");
    later.installation_identifier = other_installation();
    let replayed = store.admit(&later, SECOND_INSTANT).expect("a replay");
    assert!(matches!(replayed, AdmissionOutcome::Replayed(_)), "the same work replays");
    assert_eq!(
        replayed.summary().installation_identifier,
        installation(),
        "and a later installation replaying it does not restate who admitted it"
    );
}

#[test]
fn an_admission_that_does_not_commit_leaves_no_row_at_all() {
    let directory = tempfile::tempdir().expect("a directory");
    let path = directory.path().join("operations.sqlite3");
    let digest = partition(FIRST_PRINCIPAL);
    {
        let database = OperationDatabase::open(&path, settings()).expect("a database");
        let transaction = database.connection().unchecked_transaction().expect("a transaction");
        transaction
            .execute(
                "INSERT INTO operation \
                 (author_target_identity, author_target_identity_digest, canonical_command, \
                  command_fingerprint, command_wire_name, daemon_runtime_contract_digest, \
                  enqueue_sequence, installation_identifier, lifecycle_state, \
                  operation_identifier, operation_revision, recorded_at_unix_milliseconds, \
                  selected_environment_revision) \
                 VALUES ('opaque', ?, '{}', 'f', 'query_paths', 'd', 1, ?, 'queued', \
                         'operation-1', 1, 0, 'revision-1')",
                rusqlite::params![digest, installation().as_text()],
            )
            .expect("a staged row");
        drop(transaction);
    }

    let store = repository(&path);
    assert!(
        store.read(&digest, "operation-1").expect("a read").is_none(),
        "an admission interrupted before its commit left nothing behind"
    );
    assert!(
        store.reconstruct(&digest).expect("a reconstruction").is_empty(),
        "and nothing for a restart to reconstruct"
    );
}
