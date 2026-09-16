//! Database identity, settings and physical-inventory regressions.

use super::{OperationDatabase, RequiredSettings, StartupDatabaseBinding};
use slingshot_domain::daemon_runtime_contract::DaemonRuntimeContract;
use slingshot_domain::installation::{IDENTIFIER_CHARACTERS, InstallationIdentifier};

const TEST_PAGE_BYTES: u64 = 4096;
const TEST_DATABASE_PAGES: u64 = 262_144;
const TEST_BUSY_TIMEOUT_MILLISECONDS: u64 = 5000;
const PARTITION_DIMENSIONS: usize = 4;
const INSTALLATION_DIMENSION: usize = 3;
const ADMISSION_TIME: i64 = 123;
const SETTLEMENT_TIME: u64 = 124;

fn settings() -> RequiredSettings {
    RequiredSettings {
        page_bytes: TEST_PAGE_BYTES,
        database_pages: TEST_DATABASE_PAGES,
        busy_timeout_milliseconds: TEST_BUSY_TIMEOUT_MILLISECONDS,
    }
}

#[test]
fn bound_startup_refuses_each_foreign_partition_before_changes() {
    use crate::operation_repository::{AdmissionRequest, OperationRepository};
    use slingshot_domain::command_fingerprint::{CommandFingerprint, FingerprintInput};
    for dimension in 0..PARTITION_DIMENSIONS {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("operations.sqlite3");
        let identity = InstallationIdentifier::parse(&"a".repeat(IDENTIFIER_CHARACTERS)).unwrap();
        let database = OperationDatabase::open(&path, settings()).unwrap();
        database.record_installation_identifier(&identity, ADMISSION_TIME).unwrap();
        let mut partition = ["target", "revision", "contract"];
        if dimension < INSTALLATION_DIMENSION {
            partition[dimension] = "foreign";
        }
        let retained_installation = if dimension == INSTALLATION_DIMENSION {
            "b".repeat(IDENTIFIER_CHARACTERS)
        } else {
            identity.as_text().to_owned()
        };
        let repository = OperationRepository::new(database);
        let canonical_command = r#"{"root_path":"/content/example"}"#.to_owned();
        repository
            .admit(
                &AdmissionRequest {
                    author_target_identity: "identity".to_owned(),
                    author_target_identity_digest: partition[0].to_owned(),
                    caller_identity: None,
                    command_fingerprint: CommandFingerprint::derive(&FingerprintInput {
                        author_target_identity_digest: partition[0].to_owned(),
                        canonical_command: canonical_command.clone(),
                        command_wire_name: "query_paths".to_owned(),
                        command_semantic_contract_version: "1.0.0".to_owned(),
                        selected_environment_revision: partition[1].to_owned(),
                    })
                    .unwrap(),
                    canonical_command,
                    command_wire_name: "query_paths".to_owned(),
                    daemon_runtime_contract_digest: partition[2].to_owned(),
                    installation_identifier: InstallationIdentifier::parse(&retained_installation)
                        .unwrap(),
                    operation_identifier: "operation".to_owned(),
                    selected_environment_revision: partition[1].to_owned(),
                    workflow_correlation_identifier: None,
                },
                ADMISSION_TIME as u64,
            )
            .unwrap();
        drop(repository);
        let before = std::fs::read(&path).unwrap();
        let binding = StartupDatabaseBinding {
            installation: &identity,
            target: "target",
            revision: "revision",
            runtime_contract: "contract",
        };
        assert!(OperationDatabase::reopen_bound(&path, settings(), binding).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        let database = OperationDatabase::open_live(&path, settings()).unwrap();
        assert_eq!(
            database.unfinished_partitions().unwrap(),
            vec![(partition[0].to_owned(), partition[1].to_owned(), partition[2].to_owned(),)]
        );
        let repository = OperationRepository::new(database);
        repository
            .settle_success(
                partition[0],
                "operation",
                &slingshot_domain::operation::SuccessfulSettlement {
                    artifacts: Vec::new(),
                    inline_result: Some("{}".to_owned()),
                    expected_lifecycle_state:
                        slingshot_domain::operation::OperationLifecycleState::Queued,
                    expected_revision: 1,
                    settled_at_unix_milliseconds: SETTLEMENT_TIME,
                },
            )
            .unwrap();
        drop(repository);
        assert!(OperationDatabase::reopen_bound(&path, settings(), binding).is_ok());
    }
}

#[test]
fn authorizer_refuses_file_escaping_and_temporary_sql_before_effect() {
    let root = tempfile::tempdir().expect("a temporary directory");
    let database = OperationDatabase::open(&root.path().join("operations.sqlite3"), settings())
        .expect("a migrated database");
    let attachment = root.path().join("attachment.sqlite3");
    assert!(
        database
            .connection()
            .execute("ATTACH DATABASE ? AS outside", [attachment.to_string_lossy()])
            .is_err(),
        "the authorizer refuses an attachment while SQLite prepares it"
    );
    assert!(!attachment.exists(), "the refused attachment creates no file");
    assert!(
        database.connection().execute_batch("CREATE TEMP TABLE forbidden (value INTEGER)").is_err(),
        "the authorizer refuses temporary database objects"
    );
    assert!(
        database.connection().execute_batch("CREATE TABLE forbidden (value INTEGER)").is_err(),
        "the authorizer refuses permanent schema changes after migration"
    );
    assert!(
        database.connection().execute_batch("PRAGMA temp_store_directory = '/tmp'").is_err(),
        "the authorizer refuses an ambient temporary-directory override"
    );
    assert!(
        database.connection().execute_batch("PRAGMA user_version = 99").is_err(),
        "the authorizer refuses write pragmas after migration"
    );
}

#[test]
fn settings_are_read_back_on_the_product_connection() {
    let root = tempfile::tempdir().expect("a temporary directory");
    let settings = settings();
    let database = OperationDatabase::open(&root.path().join("operations.sqlite3"), settings)
        .expect("a migrated database");
    let read_integer = |pragma: &str| {
        database
            .connection()
            .query_row(&format!("PRAGMA {pragma}"), [], |row| row.get::<_, i64>(0))
            .expect("the pragma reads")
    };
    let read_text = |pragma: &str| {
        database
            .connection()
            .query_row(&format!("PRAGMA {pragma}"), [], |row| row.get::<_, String>(0))
            .expect("the pragma reads")
    };
    assert_eq!(
        read_integer("page_size"),
        i64::try_from(settings.page_bytes).expect("a page count")
    );
    assert_eq!(
        read_integer("max_page_count"),
        i64::try_from(settings.database_pages).expect("a page count")
    );
    assert_eq!(
        read_integer("busy_timeout"),
        i64::try_from(settings.busy_timeout_milliseconds).expect("a timeout")
    );
    assert_eq!(read_text("journal_mode"), "wal");
    assert_eq!(read_integer("synchronous"), 2);
    assert_eq!(read_integer("foreign_keys"), 1);
    assert_eq!(read_integer("temp_store"), 2);
    assert_eq!(
        read_integer("wal_autocheckpoint"),
        i64::try_from(
            DaemonRuntimeContract::embedded().limit("maximum_sqlite_write_ahead_log_frames")
        )
        .expect("a frame limit")
    );
    assert_eq!(
        read_integer("journal_size_limit"),
        i64::try_from(
            DaemonRuntimeContract::embedded().formula("maximum_sqlite_write_ahead_log_bytes")
        )
        .expect("a WAL byte limit")
    );
    assert!(database.require_compile_options().is_ok());
}

#[test]
fn restart_refuses_an_uninventoried_sqlite_sidecar() {
    let root = tempfile::tempdir().expect("a temporary directory");
    let path = root.path().join("operations.sqlite3");
    drop(OperationDatabase::open(&path, settings()).expect("a migrated database"));
    std::fs::write(root.path().join("operations.sqlite3-journal"), b"unexpected")
        .expect("the adversarial sidecar exists");
    let outcome = OperationDatabase::open(&path, settings());
    assert!(
        matches!(outcome, Err(super::DatabaseFailure::PhysicalInventoryRefused(_))),
        "an undeclared SQLite sidecar is refused before service: {outcome:?}"
    );
}

#[test]
fn restart_refuses_a_permitted_object_over_the_physical_budget() {
    let root = tempfile::tempdir().expect("a temporary directory");
    let path = root.path().join("operations.sqlite3");
    drop(OperationDatabase::open(&path, settings()).expect("a migrated database"));
    let replacement = std::fs::File::create(root.path().join("operations.sqlite3.replacement"))
        .expect("the adversarial replacement exists");
    replacement
        .set_len(DaemonRuntimeContract::embedded().formula("maximum_sqlite_physical_bytes") + 1)
        .expect("a sparse over-budget fixture");
    let outcome = OperationDatabase::open(&path, settings());
    assert!(
        matches!(outcome, Err(super::DatabaseFailure::PhysicalInventoryRefused(_))),
        "an over-budget SQLite object is refused before service: {outcome:?}"
    );
}
