//! One identity, one ledger, one record, and a great many refusals.
//!
//! Remote subscriptions are derived from the installation identifier, so a
//! daemon that found the record missing and quietly made a new one would strand
//! every one of them. The startup table is written the way it is to make that
//! impossible by construction: creating an identity is permitted in exactly one
//! situation - a state root with nothing in it at all - and every other missing,
//! unreadable, or mismatched combination refuses and leaves the bytes alone.
//!
//! The record is replaced atomically and read back through the handle it was
//! opened with, and both are exercised against a real directory rather than
//! described.

use serde_json::Value;
use slingshot_domain::installation::{
    InstallationIdentifier, InstallationRecord, ObservedState, StartupDisposition,
    TargetRegistration, classify_startup,
};
use slingshot_storage::installation_state::{
    InstallationState, InstallationStateFailure, LOCK_FILE_NAME, RECORD_FILE_NAME,
};

/// Startup vectors this test reads.
const STARTUP: &str = include_str!("fixtures/installation-state/startup.jsonl");

/// Ledger vectors this test reads.
const LEDGER: &str = include_str!("fixtures/installation-state/ledger.jsonl");

/// Characters an identifier is rendered with.
const IDENTIFIER_CHARACTERS: usize = 64;

/// Reads one row's string member.
fn text<'row>(row: &'row Value, member: &str) -> &'row str {
    row[member].as_str().unwrap_or_else(|| panic!("{member} is a string in {row}"))
}

/// Returns every row of one fixture.
fn rows(fixture: &str) -> Vec<Value> {
    fixture
        .lines()
        .map(|line| serde_json::from_str(line).expect("every fixture line is one object"))
        .collect()
}

/// Writes one file the way the store itself would, then corrupts its contents.
///
/// The permissions matter: a record anyone could read is refused before its
/// bytes are looked at, and this test is about the bytes.
fn write_private(path: &std::path::Path, contents: &[u8]) {
    std::fs::write(path, contents).expect("written");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;

        /// Permission bits a private file carries.
        const OWNER_ONLY: u32 = 0o600;

        std::fs::set_permissions(path, std::fs::Permissions::from_mode(OWNER_ONLY))
            .expect("permissions set");
    }
}

/// Returns one identifier of repeated `octet` characters.
fn identifier(octet: char) -> InstallationIdentifier {
    InstallationIdentifier::parse(&octet.to_string().repeat(IDENTIFIER_CHARACTERS))
        .expect("a legal identifier")
}

#[test]
fn every_startup_combination_lands_where_the_fixture_says() {
    let vectors = rows(STARTUP);
    assert!(vectors.len() >= 11, "every combination that can arise on a real disk");
    for row in &vectors {
        let registration = match row["registration"].as_str() {
            None => None,
            Some("initializing") => Some(TargetRegistration::Initializing),
            Some(other) => {
                assert_eq!(other, "registered", "a registration this ledger has");
                Some(TargetRegistration::Registered)
            }
        };
        let observed = ObservedState {
            database_present: row["database_present"].as_bool().expect("a Boolean"),
            database_identifier_matches: row["database_identifier_matches"]
                .as_bool()
                .expect("a Boolean"),
            record_present: row["record_present"].as_bool().expect("a Boolean"),
            record_readable: row["record_readable"].as_bool().expect("a Boolean"),
            state_root_occupied: row["state_root_occupied"].as_bool().expect("a Boolean"),
            registration,
        };
        let note = text(row, "note");
        match (text(row, "disposition"), classify_startup(observed)) {
            ("create_installation", StartupDisposition::CreateInstallation) => (),
            ("stage_target", StartupDisposition::StageTarget) => (),
            ("resume_staging", StartupDisposition::ResumeStaging) => (),
            ("proceed", StartupDisposition::Proceed) => (),
            ("refuse", StartupDisposition::Refuse { reason }) => {
                assert_eq!(reason, text(row, "reason"), "{note}");
            }
            (expected, actual) => panic!("{note}: expected {expected}, got {actual:?}"),
        }
    }
}

#[test]
fn an_identity_is_created_in_exactly_one_situation() {
    let vectors = rows(STARTUP);
    let creating: Vec<&Value> =
        vectors.iter().filter(|row| text(row, "disposition") == "create_installation").collect();
    assert_eq!(creating.len(), 1, "one situation, and it is an empty state root");
    assert!(!creating[0]["state_root_occupied"].as_bool().expect("a Boolean"));

    let missing_beside_state = ObservedState {
        database_present: true,
        database_identifier_matches: false,
        record_present: false,
        record_readable: false,
        state_root_occupied: true,
        registration: None,
    };
    let answered = classify_startup(missing_beside_state);
    assert!(
        !answered.creates_identity(),
        "a missing record beside existing state is where a replacement would strand \
         every subscription those targets already hold"
    );
    assert!(answered.changes_nothing());
}

#[test]
fn every_ledger_transition_lands_where_the_fixture_says() {
    for row in &rows(LEDGER) {
        let mut record = InstallationRecord::new(identifier('a'));
        match row["held"].as_str() {
            None => (),
            Some("initializing") => record = record.stage("namespace").expect("staging"),
            Some(_) => {
                record = record.stage("namespace").expect("staging");
                record = record.register("namespace").expect("registering");
            }
        }
        let outcome = match text(row, "action") {
            "stage" => record.stage("namespace"),
            other => {
                assert_eq!(other, "register", "an action this ledger has");
                record.register("namespace")
            }
        };
        let note = text(row, "note");
        match (row["accepted"].as_bool(), outcome) {
            (Some(true), Ok(next)) => {
                let expected = match text(row, "result") {
                    "initializing" => TargetRegistration::Initializing,
                    _ => TargetRegistration::Registered,
                };
                assert_eq!(next.registration("namespace"), Some(expected), "{note}");
            }
            (Some(false), Err(_)) => (),
            (_, outcome) => panic!("{note}: answered {outcome:?}"),
        }
    }
}

#[test]
fn a_record_survives_being_written_and_read_back() {
    let root = tempfile::tempdir().expect("a temporary directory");
    let state = InstallationState::at(root.path());
    assert!(matches!(state.read(), Err(InstallationStateFailure::Absent)));
    assert!(!state.state_root_occupied(), "nothing is there yet");

    let record = InstallationRecord::new(identifier('b'))
        .stage("slingshot-a1b2c3d4")
        .expect("staging")
        .register("slingshot-a1b2c3d4")
        .expect("registering");
    state.replace(&record).expect("the record is published");
    assert!(state.state_root_occupied(), "and now something is");

    let read = state.read().expect("the record reads back");
    assert_eq!(read, record, "byte for byte the same facts");
    assert_eq!(read.registration("slingshot-a1b2c3d4"), Some(TargetRegistration::Registered));
    assert!(state.record_path().ends_with(RECORD_FILE_NAME));
    assert!(state.lock_path().ends_with(LOCK_FILE_NAME));
    assert_ne!(state.record_path(), state.lock_path(), "the lock is not the record");
}

#[test]
fn replacing_a_record_leaves_no_partial_one_behind() {
    let root = tempfile::tempdir().expect("a temporary directory");
    let state = InstallationState::at(root.path());
    let first = InstallationRecord::new(identifier('c'));
    state.replace(&first).expect("published");
    let second = first.stage("slingshot-a1b2c3d4").expect("staging");
    state.replace(&second).expect("published again");

    assert_eq!(state.read().expect("reads"), second, "the whole new record, not half of it");
    let leftovers: Vec<String> = std::fs::read_dir(root.path())
        .expect("the directory reads")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains("staging"))
        .collect();
    assert!(leftovers.is_empty(), "no half-written record is left where a reader could find it");
}

#[test]
fn a_corrupt_record_is_refused_rather_than_replaced() {
    let root = tempfile::tempdir().expect("a temporary directory");
    let state = InstallationState::at(root.path());
    write_private(&state.record_path(), b"{ this is not a record");
    let before = std::fs::read(state.record_path()).expect("read");
    assert!(matches!(state.read(), Err(InstallationStateFailure::Unreadable(_))));
    assert_eq!(
        std::fs::read(state.record_path()).expect("read"),
        before,
        "every byte is where it was"
    );
}

#[test]
fn a_record_from_another_format_is_refused() {
    let root = tempfile::tempdir().expect("a temporary directory");
    let state = InstallationState::at(root.path());
    let record = InstallationRecord {
        format: "slingshot.installation-state/2".to_owned(),
        ..InstallationRecord::new(identifier('d'))
    };
    state.replace(&record).expect("published");
    assert!(matches!(state.read(), Err(InstallationStateFailure::Unsupported(_))));
}

#[test]
fn an_identifier_is_sixty_four_lowercase_hexadecimal_characters() {
    assert!(InstallationIdentifier::parse(&"a".repeat(IDENTIFIER_CHARACTERS)).is_ok());
    for wrong in [
        "A".repeat(IDENTIFIER_CHARACTERS),
        "a".repeat(IDENTIFIER_CHARACTERS - 1),
        "a".repeat(IDENTIFIER_CHARACTERS + 1),
        format!("g{}", "a".repeat(IDENTIFIER_CHARACTERS - 1)),
        String::new(),
    ] {
        assert!(InstallationIdentifier::parse(&wrong).is_err(), "{wrong} is not one");
    }
}
