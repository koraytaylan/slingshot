//! The production claim transaction rotates durable producers, including reopen.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use slingshot_domain::command_fingerprint::{CommandFingerprint, FingerprintInput};
use slingshot_domain::daemon_runtime_contract::DaemonRuntimeContract;
use slingshot_domain::installation::InstallationIdentifier;
use slingshot_domain::selected_command_contract_identity::SelectedCommandContractIdentity;
use slingshot_storage::database::{OperationDatabase, RequiredSettings};
use slingshot_storage::operation::scheduler_claim::{self, QueuedCandidate, SelectedClaim};
use slingshot_storage::operation_repository::{AdmissionRequest, OperationRepository};

/// Target shared by the competing producers in this fixture.
const TARGET: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
/// Wall evidence is fixed; readiness comes from the production selection callback.
const NOW: u64 = 1_000;
/// Claims stay live while independent calls and connections select other rows.
const LEASE: u64 = 100_000;
/// The one scheduler instance's fence.
const FENCE: u64 = 7;
/// A newer owner planted to verify that a stale claim rotates nothing.
const NEWER_FENCE: u64 = 8;
/// The first persisted operation revision.
const INITIAL_REVISION: u64 = 1;

/// Opens exactly the bounded SQLite configuration the daemon uses.
fn database(path: &Path) -> OperationDatabase {
    let limits = DaemonRuntimeContract::embedded();
    OperationDatabase::open(
        path,
        RequiredSettings {
            page_bytes: limits.limit("sqlite_page_bytes"),
            database_pages: limits.limit("maximum_sqlite_database_pages"),
            busy_timeout_milliseconds: limits.limit("database_busy_timeout_milliseconds"),
        },
    )
    .unwrap()
}

/// Each method opens a new connection; scheduling history must survive closure.
struct Fixture {
    /// Owns the durable database for the fixture's entire lifetime.
    root: tempfile::TempDir,
}

impl Fixture {
    /// Creates an isolated target namespace.
    fn new() -> Self {
        let held = Self { root: tempfile::tempdir().unwrap() };
        drop(database(&held.path()));
        held
    }

    /// The same file is reopened for every admission and claim.
    fn path(&self) -> PathBuf {
        self.root.path().join("fairness.sqlite")
    }

    /// Admits through the real idempotent repository API.
    fn admit(&self, identifier: &str, producer: Option<&str>) {
        let command = r#"{"root_path":"/content/example"}"#.to_owned();
        let installed = SelectedCommandContractIdentity::installed("query_paths").unwrap();
        let request = AdmissionRequest {
            author_target_identity: TARGET.to_owned(),
            author_target_identity_digest: TARGET.to_owned(),
            caller_identity: producer.map(str::to_owned),
            canonical_command: command.clone(),
            command_fingerprint: CommandFingerprint::derive(&FingerprintInput {
                author_target_identity_digest: TARGET.to_owned(),
                canonical_command: command,
                command_wire_name: "query_paths".to_owned(),
                command_semantic_contract_version: installed.command_semantic_contract_version,
                selected_environment_revision: TARGET.to_owned(),
            })
            .unwrap(),
            command_wire_name: "query_paths".to_owned(),
            daemon_runtime_contract_digest: DaemonRuntimeContract::embedded_digest()
                .as_text()
                .to_owned(),
            installation_identifier: InstallationIdentifier::parse(TARGET).unwrap(),
            operation_identifier: identifier.to_owned(),
            selected_environment_revision: TARGET.to_owned(),
            workflow_correlation_identifier: None,
        };
        OperationRepository::new(database(&self.path())).admit(&request, NOW).unwrap();
    }

    /// One available execution slot per call, using the ordered ready snapshot.
    fn claim(&self) -> Option<String> {
        claim(&self.path(), first).map(|held| held.operation_identifier)
    }

    /// Independent inspection of committed turn history, including ordering.
    fn turns(&self) -> Vec<(String, i64)> {
        let connection = rusqlite::Connection::open(self.path()).unwrap();
        connection
            .prepare("SELECT producer_key, turn_sequence FROM producer_turn ORDER BY turn_sequence")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    }
}

/// Every row in these fixtures has no retry delay.
fn first(candidates: &[QueuedCandidate]) -> Option<usize> {
    candidates.iter().position(|_| true)
}

/// Invokes the same transaction that the daemon supplies its retry clock to.
fn claim(
    path: &Path,
    choose: impl FnOnce(&[QueuedCandidate]) -> Option<usize>,
) -> Option<SelectedClaim> {
    scheduler_claim::claim_next_queued_with(&database(path), TARGET, FENCE, LEASE, NOW, choose)
        .unwrap()
}

#[test]
fn one_free_slot_rotates_producers_across_reopen_and_preserves_enqueue_order() {
    let fixture = Fixture::new();
    for (identifier, producer) in [
        ("z-first", "z"),
        ("z-second", "z"),
        ("z-third", "z"),
        ("a-first", "a"),
        ("a-second", "a"),
        ("m-first", "m"),
    ] {
        fixture.admit(identifier, Some(producer));
    }
    for expected in ["z-first", "a-first", "m-first", "z-second", "a-second", "z-third"] {
        assert_eq!(fixture.claim().as_deref(), Some(expected));
    }
    assert!(fixture.claim().is_none());
    assert!(fixture.turns().is_empty(), "idle producer history is pruned");
}

#[test]
fn a_new_producer_joins_the_tail_and_replay_cannot_transfer_ownership() {
    let fixture = Fixture::new();
    for (identifier, producer) in [("a-one", "a"), ("a-two", "a"), ("b-one", "b")] {
        fixture.admit(identifier, Some(producer));
    }
    assert_eq!(fixture.claim().as_deref(), Some("a-one"));
    fixture.admit("a-two", Some("other-producer"));
    fixture.admit("c-one", Some("c"));
    for expected in ["b-one", "a-two", "c-one"] {
        assert_eq!(fixture.claim().as_deref(), Some(expected));
    }
}

#[test]
fn the_default_queue_is_distinct_and_an_idle_producer_rejoins_at_the_tail() {
    let fixture = Fixture::new();
    fixture.admit("default-one", None);
    fixture.admit("empty-one", Some(""));
    fixture.admit("named-one", Some("named"));
    fixture.admit("named-two", Some("named"));
    assert_eq!(fixture.claim().as_deref(), Some("default-one"));
    assert_eq!(fixture.claim().as_deref(), Some("empty-one"));
    fixture.admit("default-two", None);
    for expected in ["named-one", "default-two", "named-two"] {
        assert_eq!(fixture.claim().as_deref(), Some(expected));
    }
}

#[test]
fn no_ready_operation_and_a_fenced_claim_rotate_nothing() {
    let fixture = Fixture::new();
    fixture.admit("a-one", Some("a"));
    fixture.admit("b-one", Some("b"));
    assert!(claim(&fixture.path(), |_| None).is_none());
    let before = fixture.turns();
    assert!(claim(&fixture.path(), |_| None).is_none());
    assert_eq!(fixture.turns(), before);
    assert_eq!(
        scheduler_claim::claim(
            &database(&fixture.path()),
            TARGET,
            "a-one",
            "queued",
            INITIAL_REVISION,
            NEWER_FENCE,
            NOW,
            NOW,
        )
        .unwrap(),
        scheduler_claim::ClaimOutcome::Claimed
    );
    assert!(fixture.claim().is_none(), "an older fence cannot claim the first row");
    assert_eq!(fixture.turns(), before);
}

#[test]
fn turn_overflow_rolls_back_the_operation_claim() {
    let fixture = Fixture::new();
    fixture.admit("a-one", Some("a"));
    assert!(claim(&fixture.path(), |_| None).is_none());
    rusqlite::Connection::open(fixture.path())
        .unwrap()
        .execute("UPDATE producer_turn SET turn_sequence = ?", [i64::MAX])
        .unwrap();
    let held = database(&fixture.path());
    assert!(
        scheduler_claim::claim_next_queued_with(&held, TARGET, FENCE, LEASE, NOW, first).is_err()
    );
    assert_eq!(
        scheduler_claim::facts(&held, TARGET, "a-one").unwrap().unwrap().scheduler_fence,
        None
    );
    assert_eq!(fixture.turns(), vec![(":a".to_owned(), i64::MAX)]);
}

#[test]
fn competing_transactions_never_claim_one_operation_twice() {
    let fixture = Fixture::new();
    fixture.admit("a-one", Some("a"));
    fixture.admit("b-one", Some("b"));
    let workers: Vec<_> = ["first", "second", "third"]
        .into_iter()
        .map(|_| {
            let path = fixture.path();
            std::thread::spawn(move || claim(&path, first).map(|held| held.operation_identifier))
        })
        .collect();
    let selected: Vec<_> =
        workers.into_iter().filter_map(|worker| worker.join().unwrap()).collect();
    let expected = BTreeSet::from(["a-one".to_owned(), "b-one".to_owned()]);
    assert_eq!(selected.len(), expected.len());
    assert_eq!(selected.into_iter().collect::<BTreeSet<_>>(), expected);
}

#[test]
fn a_producer_returning_between_ticks_cannot_keep_its_retired_turn() {
    let fixture = Fixture::new();
    fixture.admit("a-one", Some("a"));
    fixture.admit("b-one", Some("b"));
    assert_eq!(fixture.claim().as_deref(), Some("a-one"));
    fixture.admit("c-one", Some("c"));
    fixture.admit("a-two", Some("a"));
    for expected in ["b-one", "c-one", "a-two"] {
        assert_eq!(fixture.claim().as_deref(), Some(expected));
    }
}

#[test]
fn existing_version_thirteen_operations_gain_turns_without_rewriting_admissions() {
    const PREVIOUS_SCHEMA_VERSION: u32 = 13;
    let fixture = Fixture::new();
    fixture.admit("a-one", Some("a"));
    fixture.admit("a-two", Some("a"));
    fixture.admit("b-one", Some("b"));
    let repository = OperationRepository::new(database(&fixture.path()));
    let before = repository.read(TARGET, "a-one").unwrap();
    drop(repository);
    // Version fourteen adds only this empty table. Removing it reconstructs
    // the preceding schema while retaining actual repository admissions.
    let connection = rusqlite::Connection::open(fixture.path()).unwrap();
    connection.execute_batch("DROP TABLE producer_turn").unwrap();
    connection.pragma_update(None, "user_version", PREVIOUS_SCHEMA_VERSION).unwrap();
    drop(connection);
    let repository = OperationRepository::new(database(&fixture.path()));
    assert_eq!(repository.read(TARGET, "a-one").unwrap(), before);
    drop(repository);
    for expected in ["a-one", "b-one", "a-two"] {
        assert_eq!(fixture.claim().as_deref(), Some(expected));
    }
}
