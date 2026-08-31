//! Moving an operation forward, and refusing to move it sideways.
//!
//! Every write is a compare-and-set, so the tests here are mostly about two
//! writers: the one holding the current revision, which advances the row
//! exactly once, and the one holding a stale revision, which writes nothing
//! and is told so.

use slingshot_domain::operation::{
    LifecycleFailure, OperationFact, OperationLifecycleState, TerminalFailureDisposition,
    TerminalFailureKind,
};
use slingshot_storage::operation_repository::{RepositoryFailure, ResultDisposition};

use crate::fixtures::*;

#[test]
fn a_stale_expected_revision_cannot_write_and_a_current_one_advances_once() {
    let store = in_memory();
    let digest = partition(FIRST_PRINCIPAL);
    let first = admitted(&store, &digest);

    let advanced =
        applied(&store, &digest, first, &reaching(OperationLifecycleState::Submitting), NOW);
    assert_eq!(advanced.record.lifecycle_state, OperationLifecycleState::Submitting);
    assert_eq!(advanced.record.revision, first + 1, "one advance is one revision");

    let stale = store.apply(
        &digest,
        OPERATION,
        first,
        &reaching(OperationLifecycleState::Accepted),
        SECOND_INSTANT,
    );
    assert!(
        matches!(
            stale,
            Err(RepositoryFailure::RevisionMoved { expected, stored })
                if expected == first && stored == first + 1
        ),
        "a writer holding the revision before this one is told so: {stale:?}"
    );
    assert_eq!(
        store.read(&digest, OPERATION).expect("a read").expect("a row").record,
        advanced.record,
        "and wrote nothing"
    );
}

#[test]
fn recording_the_same_fact_twice_is_one_recorded_fact() {
    let store = in_memory();
    let digest = partition(FIRST_PRINCIPAL);
    let first = admitted(&store, &digest);
    let submitting = reaching(OperationLifecycleState::Submitting);

    let once = applied(&store, &digest, first, &submitting, NOW);
    let twice = applied(&store, &digest, once.record.revision, &submitting, SECOND_INSTANT);
    assert_eq!(
        twice.record.revision, once.record.revision,
        "a fact that changes nothing takes the revision nowhere"
    );
    assert_eq!(twice, once, "and leaves the row exactly as it was");
}

#[test]
fn a_transition_the_domain_refuses_is_refused_here_too() {
    let store = in_memory();
    let digest = partition(FIRST_PRINCIPAL);
    let first = admitted(&store, &digest);

    let skipped =
        store.apply(&digest, OPERATION, first, &reaching(OperationLifecycleState::Running), NOW);
    assert!(
        matches!(
            skipped,
            Err(RepositoryFailure::Lifecycle(LifecycleFailure::TransitionNotAllowed))
        ),
        "an operation moves forward one state at a time: {skipped:?}"
    );
    assert_eq!(
        store.read(&digest, OPERATION).expect("a read").expect("a row").record.revision,
        first,
        "and the refusal wrote nothing"
    );
}

#[test]
fn a_progress_note_longer_than_its_bound_is_refused_before_the_write() {
    let store = in_memory();
    let digest = partition(FIRST_PRINCIPAL);
    let first = admitted(&store, &digest);

    let largest = OperationFact::Progress { detail: "d".repeat(PROGRESS_DETAIL_BYTES) };
    let noted = applied(&store, &digest, first, &largest, NOW);
    assert_eq!(noted.record.latest_progress.as_deref().map(str::len), Some(PROGRESS_DETAIL_BYTES));

    let over = OperationFact::Progress { detail: "d".repeat(PROGRESS_DETAIL_BYTES + 1) };
    let refused = store.apply(&digest, OPERATION, noted.record.revision, &over, SECOND_INSTANT);
    assert!(
        matches!(refused, Err(RepositoryFailure::TooLong { field: "progress detail", .. })),
        "one byte further is refused: {refused:?}"
    );
    assert_eq!(
        store.read(&digest, OPERATION).expect("a read").expect("a row").record,
        noted.record,
        "and the row still holds the note that fit"
    );
}

#[test]
fn a_settled_operation_takes_no_further_fact() {
    let store = in_memory();
    let digest = partition(FIRST_PRINCIPAL);
    let first = admitted(&store, &digest);
    let ended = settling(
        TerminalFailureKind::RemoteFailed,
        TerminalFailureDisposition::AuthoritativeRemoteFailure,
    );
    let settled = applied(&store, &digest, first, &ended, NOW);

    let after = store.apply(
        &digest,
        OPERATION,
        settled.record.revision,
        &reaching(OperationLifecycleState::Running),
        SECOND_INSTANT,
    );
    assert!(
        matches!(after, Err(RepositoryFailure::Lifecycle(LifecycleFailure::AlreadyTerminal))),
        "a terminal operation takes no further fact: {after:?}"
    );
    assert_eq!(
        store.read(&digest, OPERATION).expect("a read").expect("a row"),
        settled,
        "and the settled row is exactly what it was"
    );
}

#[test]
fn a_fact_for_an_operation_that_is_not_there_names_the_identifier_it_looked_for() {
    let store = in_memory();
    let digest = partition(FIRST_PRINCIPAL);
    let missing = store.apply(
        &digest,
        "operation-nobody-admitted",
        1,
        &reaching(OperationLifecycleState::Submitting),
        NOW,
    );
    assert!(
        matches!(missing, Err(RepositoryFailure::NoSuchOperation { ref identifier })
            if identifier == "operation-nobody-admitted"),
        "{missing:?}"
    );
}

#[test]
fn reopening_reconstructs_the_partition_in_the_order_its_callers_asked() {
    let directory = tempfile::tempdir().expect("a directory");
    let path = directory.path().join("operations.sqlite3");
    let digest = partition(FIRST_PRINCIPAL);
    let elsewhere = partition(SECOND_PRINCIPAL);
    {
        let store = repository(&path);
        for index in 0..ADMITTED_OPERATIONS {
            let identifier = format!("operation-{index}");
            let command = format!("{{\"paths\":[\"/{index}\"]}}");
            store
                .admit(&request(&digest, &identifier, &command, "revision-1"), NOW)
                .expect("an admission");
        }
        store
            .admit(&request(&elsewhere, "operation-0", "{\"paths\":[\"/0\"]}", "revision-1"), NOW)
            .expect("another partition's admission");
        let ending = store.read(&digest, "operation-1").expect("a read").expect("a row");
        store
            .apply(
                &digest,
                "operation-1",
                ending.record.revision,
                &settling(
                    TerminalFailureKind::RemoteFailed,
                    TerminalFailureDisposition::AuthoritativeRemoteFailure,
                ),
                NOW,
            )
            .expect("a settlement");
    }

    let store = repository(&path);
    let held = store.reconstruct(&digest).expect("a reconstruction");
    assert_eq!(
        held.iter().map(|row| row.operation_identifier.as_str()).collect::<Vec<&str>>(),
        vec!["operation-0", "operation-1", "operation-2", "operation-3"],
        "arrival order, not identifier order and not timestamp order"
    );
    assert_eq!(
        held.iter().map(|row| row.enqueue_sequence).collect::<Vec<u64>>(),
        vec![1, 2, 3, 4],
        "and one partition counts only its own arrivals"
    );
    let outstanding: Vec<&str> = held
        .iter()
        .filter(|row| !row.record.lifecycle_state.is_terminal())
        .map(|row| row.operation_identifier.as_str())
        .collect();
    assert_eq!(
        outstanding,
        vec!["operation-0", "operation-2", "operation-3"],
        "the work still to do is every nonterminal row, in that same order"
    );
    let terminal = held.iter().find(|row| row.record.lifecycle_state.is_terminal());
    assert_eq!(
        terminal.map(|row| row.operation_identifier.as_str()),
        Some("operation-1"),
        "and the settled row is still there to be asked about"
    );
    assert_eq!(
        store.reconstruct(&elsewhere).expect("a reconstruction").len(),
        1,
        "reconstruction never reaches across a partition"
    );
}

#[test]
fn a_result_disposition_is_recorded_under_compare_and_set() {
    let store = in_memory();
    let digest = partition(FIRST_PRINCIPAL);
    let first = admitted(&store, &digest);
    assert_eq!(
        store.read(&digest, OPERATION).expect("a read").expect("a row").result_disposition,
        None,
        "an admitted operation has produced nothing yet"
    );

    let inline = disposed(&store, &digest, first, ResultDisposition::Inline);
    assert_eq!(inline.result_disposition, Some(ResultDisposition::Inline));
    assert_eq!(inline.record.revision, first + 1, "recording where it went is one revision");

    let again = disposed(&store, &digest, inline.record.revision, ResultDisposition::Inline);
    assert_eq!(again, inline, "and saying it twice says it once");

    let stale =
        store.record_result_disposition(&digest, OPERATION, first, ResultDisposition::Artifact);
    assert!(
        matches!(stale, Err(RepositoryFailure::RevisionMoved { .. })),
        "a stale writer cannot restate where the result went: {stale:?}"
    );
    assert_eq!(
        store.read(&digest, OPERATION).expect("a read").expect("a row").result_disposition,
        Some(ResultDisposition::Inline),
        "and did not"
    );
}
