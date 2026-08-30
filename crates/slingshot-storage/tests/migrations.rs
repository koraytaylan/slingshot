//! The schema, its constraints, and the columns it deliberately does not have.
//!
//! Two kinds of assertion here. The first is ordinary: every table exists,
//! every constraint refuses what it says it refuses, and reopening an already
//! current database changes nothing.
//!
//! The second is a negative inventory. Plan 0002 hands this daemon two opaque
//! values, and a schema that also had somewhere to put a user name, a
//! metascope, or a certificate would eventually have one in it. The test reads
//! the migration text back and asserts those columns are absent, which is
//! cheaper to keep true than to discover untrue.

use serde_json::Value;
use slingshot_storage::database::{
    DatabaseFailure, MIGRATIONS, OperationDatabase, REFUSED_COMPILE_OPTION, RequiredSettings,
    uses_forbidden_construct,
};
use slingshot_storage::sqlite_statement_inventory::{
    FORBIDDEN_CONSTRUCTS, STATEMENTS, is_inventoried,
};
use slingshot_storage::sqlite_vfs::{
    AMBIENT_TEMPORARY_VARIABLES, ObjectWhitelist, OpenRefusal, REFUSED_INTENTS, REFUSED_SUFFIXES,
    ReplacementSettings,
};

/// Table vectors this test reads.
const TABLES: &str = include_str!("fixtures/migrations/tables.jsonl");

/// Absent-column vectors this test reads.
const ABSENT: &str = include_str!("fixtures/migrations/absent-columns.jsonl");

/// Constraint vectors this test reads.
const CONSTRAINTS: &str = include_str!("fixtures/migrations/constraints.jsonl");

/// Bytes one page occupies, from the runtime contract.
const PAGE_BYTES: u64 = 4096;

/// Pages the database may reach, from the runtime contract.
const DATABASE_PAGES: u64 = 262_144;

/// Milliseconds a busy connection waits, from the runtime contract.
const BUSY_TIMEOUT: u64 = 5000;

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

/// Returns the settings every connection is held to.
fn settings() -> RequiredSettings {
    RequiredSettings {
        page_bytes: PAGE_BYTES,
        database_pages: DATABASE_PAGES,
        busy_timeout_milliseconds: BUSY_TIMEOUT,
    }
}

/// Returns one migrated database in memory.
fn migrated() -> OperationDatabase {
    OperationDatabase::open_in_memory(settings()).expect("a migrated database")
}

#[test]
fn an_empty_database_migrates_to_the_schema_the_fixture_describes() {
    let database = migrated();
    assert_eq!(database.schema_version().expect("a version"), 1);
    let mut statement = database
        .connection()
        .prepare("SELECT name FROM sqlite_schema WHERE type = 'table' ORDER BY name")
        .expect("the schema reads");
    let held: Vec<String> = statement
        .query_map([], |row| row.get::<_, String>(0))
        .expect("the rows read")
        .collect::<Result<Vec<String>, _>>()
        .expect("the rows read")
        .into_iter()
        .filter(|name| !name.starts_with("sqlite_"))
        .collect();
    let expected: Vec<String> =
        rows(TABLES).iter().map(|row| text(row, "table").to_owned()).collect();
    assert_eq!(held, expected, "the schema and the fixture describe one set of tables");
}

#[test]
fn no_column_holds_a_principal_a_secret_or_a_trust_decision() {
    let schema: String = MIGRATIONS.iter().map(|(_, text)| *text).collect::<Vec<&str>>().join("\n");
    let vectors = rows(ABSENT);
    assert!(vectors.len() >= 8, "every readable thing Plan 0002 keeps opaque");
    for row in &vectors {
        let column = text(row, "column");
        assert!(
            !schema.contains(column),
            "{}: {column} is somewhere it would eventually be filled in",
            text(row, "note")
        );
    }
    assert!(schema.contains("author_target_identity_digest"), "the opaque target is a column");
    assert!(schema.contains("selected_environment_revision"), "and so is the revision");
}

#[test]
fn reopening_a_current_database_changes_nothing() {
    let root = tempfile::tempdir().expect("a temporary directory");
    let path = root.path().join("operations.sqlite3");
    let first = OperationDatabase::open(&path, settings()).expect("a migrated database");
    assert_eq!(first.schema_version().expect("a version"), 1);
    drop(first);

    let second = OperationDatabase::open(&path, settings()).expect("reopened");
    assert_eq!(second.schema_version().expect("a version"), 1, "no migration ran again");
}

#[test]
fn a_schema_newer_than_this_binary_is_refused_without_being_touched() {
    let root = tempfile::tempdir().expect("a temporary directory");
    let path = root.path().join("operations.sqlite3");
    let database = OperationDatabase::open(&path, settings()).expect("a migrated database");
    database.connection().execute_batch("PRAGMA user_version = 99").expect("the version is set");
    drop(database);

    let before = std::fs::metadata(&path).expect("metadata").len();
    let outcome = OperationDatabase::open(&path, settings());
    assert!(
        matches!(outcome, Err(DatabaseFailure::SchemaTooNew { observed: 99, supported: 1 })),
        "answered {outcome:?}"
    );
    assert_eq!(
        std::fs::metadata(&path).expect("metadata").len(),
        before,
        "a database a newer binary understands is left for that binary"
    );
}

#[test]
fn every_setting_is_read_back_rather_than_wished_for() {
    let root = tempfile::tempdir().expect("a temporary directory");
    let path = root.path().join("operations.sqlite3");
    let database = OperationDatabase::open(&path, settings()).expect("a migrated database");
    let read = |pragma: &str| -> String {
        database
            .connection()
            .query_row(&format!("PRAGMA {pragma}"), [], |row| {
                row.get::<_, rusqlite::types::Value>(0)
            })
            .map(|value| match value {
                rusqlite::types::Value::Integer(number) => number.to_string(),
                rusqlite::types::Value::Text(text) => text,
                _ => String::new(),
            })
            .expect("the pragma reads")
    };
    assert_eq!(read("page_size"), PAGE_BYTES.to_string());
    assert_eq!(read("max_page_count"), DATABASE_PAGES.to_string());
    assert_eq!(read("busy_timeout"), BUSY_TIMEOUT.to_string());
    assert_eq!(read("journal_mode"), "wal", "the accounting assumes a write-ahead log");
    assert_eq!(read("synchronous"), "2", "and a commit that has actually landed");
    assert_eq!(read("foreign_keys"), "1");
    assert_eq!(read("temp_store"), "2", "temporary storage stays in memory");
    assert!(database.require_compile_options().is_ok());
    assert_eq!(
        REFUSED_COMPILE_OPTION, "TEMP_STORE=0",
        "the build that would make reading the pragma back mean nothing"
    );
}

#[test]
fn every_constraint_refuses_what_the_fixture_says_it_refuses() {
    for row in &rows(CONSTRAINTS) {
        let database = migrated();
        let accepted = apply_scenario(&database, text(row, "scenario"));
        assert_eq!(
            accepted,
            row["accepted"].as_bool().expect("a verdict"),
            "{}",
            text(row, "note")
        );
    }
}

/// Seeds one operation row.
fn seed_operation(
    connection: &rusqlite::Connection,
    target: &str,
    identifier: &str,
) -> rusqlite::Result<usize> {
    connection.execute(
        "INSERT INTO operation \
         (author_target_identity, author_target_identity_digest, canonical_command, \
          command_fingerprint, command_wire_name, daemon_runtime_contract_digest, \
          enqueue_sequence, installation_identifier, lifecycle_state, \
          operation_identifier, operation_revision, recorded_at_unix_milliseconds, \
          selected_environment_revision) \
         VALUES ('opaque', ?, '{}', 'f', 'query_paths', 'd', 1, 'i', 'queued', ?, 1, 0, 'r')",
        rusqlite::params![target, identifier],
    )
}

/// Seeds one blob the associations below can point at.
fn seed_blob(connection: &rusqlite::Connection) -> rusqlite::Result<usize> {
    connection.execute(
        "INSERT INTO artifact_blob (byte_length, content_digest, \
         recorded_at_unix_milliseconds) VALUES (1, 'c', 0)",
        [],
    )
}

/// Seeds one maintenance-result association, current or applied.
fn seed_association(
    connection: &rusqlite::Connection,
    current: i64,
    owner: Option<&str>,
) -> rusqlite::Result<usize> {
    connection.execute(
        "INSERT INTO maintenance_result_association \
         (association_revision, author_target_identity_digest, byte_length, \
          content_digest, is_current_preview, kind, maintenance_result_identifier, \
          media_type, owning_application_receipt_identifier, reviewed_source_digest) \
         VALUES (1, 'target', 1, 'c', ?, 'preview', ?, 'application/json', ?, 's')",
        rusqlite::params![
            current,
            if current == 1 { "result-current" } else { "result-applied" },
            owner
        ],
    )
}

/// Seeds one recovery fact with the certainty and kind the scenario names.
fn seed_recovery_fact(
    connection: &rusqlite::Connection,
    category: &str,
    certainty: Option<&str>,
    kind: &str,
) -> rusqlite::Result<usize> {
    connection.execute(
        "INSERT INTO recovery_fact \
         (attempt_count, author_target_identity_digest, category, \
          evidence_certainty, evidence_kind, manual_resume_eligible, \
          operation_identifier, retry_delay_milliseconds, \
          retry_observed_at_unix_milliseconds) \
         VALUES (1, 'target-a', ?, ?, ?, 0, 'operation-1', 0, 0)",
        rusqlite::params![category, certainty, kind],
    )
}

/// Applies one named scenario and returns whether the database accepted it.
///
/// The scenarios divide by the table whose constraint they exercise, so each
/// half stays legible on its own rather than one arm list carrying every
/// insertion this schema permits.
fn apply_scenario(database: &OperationDatabase, scenario: &str) -> bool {
    let connection = database.connection();
    match scenario {
        "two_current_previews" | "preview_and_application" | "applied_without_receipt" => {
            apply_maintenance_scenario(connection, scenario)
        }
        _ => apply_operation_scenario(connection, scenario),
    }
}

/// Applies one scenario about maintenance results and their receipts.
fn apply_maintenance_scenario(connection: &rusqlite::Connection, scenario: &str) -> bool {
    if seed_blob(connection).is_err() {
        return false;
    }
    match scenario {
        "two_current_previews" => {
            seed_association(connection, 1, None).is_ok()
                && connection
                    .execute(
                        "INSERT INTO maintenance_result_association \
                         (association_revision, author_target_identity_digest, byte_length, \
                          content_digest, is_current_preview, kind, \
                          maintenance_result_identifier, media_type, \
                          owning_application_receipt_identifier, reviewed_source_digest) \
                         VALUES (1, 'target', 1, 'c', 1, 'preview', 'result-second', \
                                 'application/json', NULL, 's')",
                        [],
                    )
                    .is_ok()
        }
        "preview_and_application" => {
            connection
                .execute(
                    "INSERT INTO maintenance_application_receipt \
                     (application_receipt_identifier, author_target_identity_digest, \
                      recorded_at_unix_milliseconds, reviewed_manifest_digest) \
                     VALUES ('receipt-1', 'target', 0, 'm')",
                    [],
                )
                .is_ok()
                && seed_association(connection, 1, None).is_ok()
                && seed_association(connection, 0, Some("receipt-1")).is_ok()
        }
        _ => {
            assert_eq!(
                scenario, "applied_without_receipt",
                "a maintenance scenario this test knows"
            );
            seed_association(connection, 0, None).is_ok()
        }
    }
}

/// Applies one scenario about operations and what hangs off them.
fn apply_operation_scenario(connection: &rusqlite::Connection, scenario: &str) -> bool {
    match scenario {
        "same_identifier_distinct_targets" => {
            seed_operation(connection, "target-a", "operation-1").is_ok()
                && seed_operation(connection, "target-b", "operation-1").is_ok()
        }
        "same_identifier_same_target" => {
            seed_operation(connection, "target-a", "operation-1").is_ok()
                && seed_operation(connection, "target-a", "operation-1").is_ok()
        }
        "proven_success_with_certainty" => {
            seed_operation(connection, "target-a", "operation-1").is_ok()
                && seed_recovery_fact(
                    connection,
                    "result_acquisition",
                    Some("submission_unknown"),
                    "authoritative_remote_success",
                )
                .is_ok()
        }
        "unresolved_without_certainty" => {
            seed_operation(connection, "target-a", "operation-1").is_ok()
                && seed_recovery_fact(
                    connection,
                    "ambiguous_submission",
                    None,
                    "execution_certainty",
                )
                .is_ok()
        }
        _ => {
            assert_eq!(
                scenario, "association_without_blob",
                "an operation scenario this test knows"
            );
            seed_operation(connection, "target-a", "operation-1").is_ok()
                && connection
                    .execute(
                        "INSERT INTO artifact_association \
                         (artifact_identifier, artifact_slot, \
                          author_target_identity_digest, byte_length, content_digest, \
                          media_type, operation_identifier) \
                         VALUES ('a', 'structured_result', 'target-a', 1, 'absent', \
                                 'application/json', 'operation-1')",
                        [],
                    )
                    .is_ok()
        }
    }
}

#[test]
fn every_statement_this_crate_runs_is_in_the_inventory_and_none_is_forbidden() {
    assert!(STATEMENTS.len() >= 8, "the whole database behaviour, in one list");
    for statement in STATEMENTS {
        assert!(is_inventoried(statement.text), "{}", statement.purpose);
        assert!(
            !uses_forbidden_construct(statement.text),
            "{}: uses a construct this crate may never run",
            statement.purpose
        );
        assert_eq!(
            statement.text.matches('?').count(),
            statement.parameters,
            "{}: the inventory counts its own parameters",
            statement.purpose
        );
        assert!(!statement.purpose.is_empty());
    }
    for construct in FORBIDDEN_CONSTRUCTS {
        assert!(
            uses_forbidden_construct(&format!("SELECT 1; {construct} x")),
            "{construct} is caught"
        );
    }
    assert!(!is_inventoried("SELECT * FROM operation"), "a statement nobody reviewed");
}

#[test]
fn only_four_objects_are_permitted_and_every_other_open_is_refused() {
    let root = std::path::Path::new("/state/slingshot-a1b2c3d4");
    let whitelist = ObjectWhitelist::for_database(root, "operations.sqlite3");
    assert_eq!(
        whitelist.permitted_names(),
        vec![
            "operations.sqlite3",
            "operations.sqlite3-wal",
            "operations.sqlite3-shm",
            "operations.sqlite3.replacement",
        ]
    );
    for name in whitelist.permitted_names() {
        assert_eq!(whitelist.require_permitted(&root.join(&name), "main"), Ok(()), "{name}");
    }
    for suffix in REFUSED_SUFFIXES {
        let path = root.join(format!("operations.sqlite3{suffix}"));
        assert_eq!(
            whitelist.require_permitted(&path, "main"),
            Err(OpenRefusal::NotAPermittedObject),
            "{suffix} holds content this daemon accounts for nowhere"
        );
    }
    for intent in REFUSED_INTENTS {
        assert_eq!(
            whitelist.require_permitted(&whitelist.main_path(), intent),
            Err(OpenRefusal::RefusedIntent((*intent).to_owned())),
            "{intent} is refused wherever somebody proposes to put it"
        );
    }
    assert_eq!(
        whitelist.require_permitted(std::path::Path::new("/tmp/operations.sqlite3"), "main"),
        Err(OpenRefusal::OutsideStateRoot)
    );
    assert_eq!(
        whitelist.require_permitted(&root.join("subdirectory/operations.sqlite3"), "main"),
        Err(OpenRefusal::OutsideStateRoot)
    );
}

#[test]
fn a_replacement_is_built_with_nothing_beside_it() {
    let pragmas = ReplacementSettings::pragmas();
    assert_eq!(
        pragmas,
        vec![("journal_mode", "OFF"), ("locking_mode", "EXCLUSIVE"), ("temp_store", "MEMORY")],
        "journalling off is what stops SQLite from wanting a sidecar the whitelist \
         would refuse anyway"
    );
    assert!(AMBIENT_TEMPORARY_VARIABLES.contains(&"SQLITE_TMPDIR"));
    assert!(AMBIENT_TEMPORARY_VARIABLES.contains(&"TMPDIR"));
    assert_eq!(AMBIENT_TEMPORARY_VARIABLES.len(), 4, "every ambient root SQLite consults");
}
