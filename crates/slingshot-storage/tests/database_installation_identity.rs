//! Durable installation identity is read explicitly and recorded only once.

use slingshot_domain::installation::InstallationIdentifier;
use slingshot_storage::database::{OperationDatabase, RequiredSettings, StartupDatabaseBinding};

fn settings() -> RequiredSettings {
    RequiredSettings { page_bytes: 4096, database_pages: 262_144, busy_timeout_milliseconds: 5000 }
}

#[test]
fn identity_is_absent_until_recorded_and_cannot_be_replaced() {
    let database = OperationDatabase::open_in_memory(settings()).unwrap();
    let original = InstallationIdentifier::parse(&"a".repeat(64)).unwrap();
    let replacement = InstallationIdentifier::parse(&"b".repeat(64)).unwrap();
    assert_eq!(database.installation_identifier().unwrap(), None);
    database.record_installation_identifier(&original, 123).unwrap();
    assert_eq!(database.installation_identifier().unwrap(), Some(original.clone()));
    assert!(database.record_installation_identifier(&original, 456).is_err());
    assert!(database.record_installation_identifier(&replacement, 456).is_err());
    assert_eq!(database.installation_identifier().unwrap(), Some(original));
}

#[test]
fn restart_reads_the_same_identity_without_reinitializing_it() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("operations.sqlite3");
    let identity = InstallationIdentifier::parse(&"c".repeat(64)).unwrap();
    {
        let database = OperationDatabase::open(&path, settings()).unwrap();
        database.record_installation_identifier(&identity, 123).unwrap();
    }
    for live in [true, false] {
        let database = if live {
            OperationDatabase::open_live(&path, settings())
        } else {
            OperationDatabase::open(&path, settings())
        }
        .unwrap();
        assert_eq!(database.installation_identifier().unwrap(), Some(identity.clone()));
        assert!(database.record_installation_identifier(&identity, 456).is_err());
    }
}

#[test]
fn bound_reopen_refuses_missing_unidentified_and_foreign_databases() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("operations.sqlite3");
    let identity = InstallationIdentifier::parse(&"a".repeat(64)).unwrap();
    let foreign = InstallationIdentifier::parse(&"b".repeat(64)).unwrap();
    let binding = StartupDatabaseBinding {
        installation: &identity,
        target: "target",
        revision: "revision",
        runtime_contract: "contract",
    };
    assert!(OperationDatabase::reopen_bound(&path, settings(), binding).is_err());
    assert!(!path.exists());
    drop(OperationDatabase::open(&path, settings()).unwrap());
    assert!(OperationDatabase::reopen_bound(&path, settings(), binding).is_err());
    let database = OperationDatabase::open_live(&path, settings()).unwrap();
    assert_eq!(database.installation_identifier().unwrap(), None);
    database.record_installation_identifier(&identity, 123).unwrap();
    drop(database);
    let before = std::fs::read(&path).unwrap();
    assert!(
        OperationDatabase::reopen_bound(
            &path,
            settings(),
            StartupDatabaseBinding { installation: &foreign, ..binding }
        )
        .is_err()
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
    let reopened = OperationDatabase::reopen_bound(&path, settings(), binding).unwrap();
    assert_eq!(reopened.installation_identifier().unwrap(), Some(identity));
}
