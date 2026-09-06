//! Filesystem-aware recovery of already-approved cleanup receipts.

use super::*;
use crate::artifact_store::{ArtifactStore, CONTENT_DIRECTORY};
use crate::database::RequiredSettings;

fn seed(database: &OperationDatabase, target: &str, digest: &str) {
    database
        .connection()
        .execute(
            statement_text("record one maintenance-application receipt"),
            rusqlite::params!["receipt", target, 123, 1, "reviewed"],
        )
        .unwrap();
    database
        .connection()
        .execute(
            statement_text("record one maintenance artifact cleanup item"),
            rusqlite::params!["receipt", target, digest],
        )
        .unwrap();
    database
        .connection()
        .execute("INSERT INTO artifact_blob VALUES (5, ?, 123)", [digest])
        .unwrap();
}

fn private_file(path: &std::path::Path) {
    use std::io::Write as _;
    let mut options = std::fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    options.open(path).unwrap().write_all(b"bytes").unwrap();
}

#[test]
fn recovery_deletes_only_approved_unreferenced_files_and_is_restart_safe() {
    for mode in ["present", "already-unlinked", "referenced", "blocked"] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("operations.sqlite3");
        let settings = RequiredSettings {
            page_bytes: 4096,
            database_pages: 262144,
            busy_timeout_milliseconds: 5000,
        };
        let database = OperationDatabase::open(&path, settings).unwrap();
        let store_root = root.path().join("artifacts");
        let store = ArtifactStore::open(&store_root).unwrap();
        let digest = "a".repeat(64);
        let foreign = "b".repeat(64);
        seed(&database, "selected", &digest);
        seed(&database, "foreign", &foreign);
        let content = store_root.join(CONTENT_DIRECTORY).join(&digest);
        let foreign_content = store_root.join(CONTENT_DIRECTORY).join(&foreign);
        private_file(&foreign_content);
        match mode {
            "already-unlinked" => {}
            "blocked" => std::fs::create_dir(&content).unwrap(),
            _ => private_file(&content),
        }
        if mode == "referenced" {
            database
                .connection()
                .execute(
                    "INSERT INTO artifact_publication VALUES ('producer', 'artifact', ?, 123)",
                    [&digest],
                )
                .unwrap();
        }
        drop(database);
        let database = OperationDatabase::open(&path, settings).unwrap();
        let recovered = recover_pending_cleanup(&database, &store, "selected");
        if mode == "blocked" {
            assert!(matches!(recovered, Err(MaintenanceFailure::Artifact(_))));
            assert_eq!(
                receipt(&database, "selected", "receipt").unwrap().unwrap().stage,
                ReceiptStage::DatabaseApplied
            );
            assert!(content.is_dir());
            std::fs::remove_dir(&content).unwrap();
        } else if mode == "referenced" {
            assert_eq!(recovered.unwrap()[0].stage, ReceiptStage::DatabaseApplied);
            assert!(content.exists());
            database
                .connection()
                .execute(
                    "DELETE FROM artifact_publication WHERE publication_identifier = 'producer'",
                    [],
                )
                .unwrap();
        } else {
            assert_eq!(recovered.unwrap()[0].stage, ReceiptStage::Completed);
            assert!(!content.exists());
        }
        let recovered = recover_pending_cleanup(&database, &store, "selected").unwrap();
        assert!(recovered.iter().all(|receipt| receipt.stage == ReceiptStage::Completed));
        assert!(!content.exists());
        assert!(recover_pending_cleanup(&database, &store, "selected").unwrap().is_empty());
        assert_eq!(std::fs::read(&foreign_content).unwrap(), b"bytes");
        assert_eq!(
            receipt(&database, "foreign", "receipt").unwrap().unwrap().stage,
            ReceiptStage::DatabaseApplied
        );
        let rows: i64 = database
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM artifact_blob WHERE content_digest = ?",
                [&digest],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(rows, 0);
    }
}

#[test]
#[cfg(unix)]
fn recovery_refuses_linked_content_without_clearing_intent() {
    for hard in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let database = OperationDatabase::open_in_memory(RequiredSettings {
            page_bytes: 4096,
            database_pages: 262144,
            busy_timeout_milliseconds: 5000,
        })
        .unwrap();
        let store = ArtifactStore::open(root.path()).unwrap();
        let digest = "c".repeat(64);
        seed(&database, "selected", &digest);
        let source = root.path().join("must-remain");
        private_file(&source);
        let linked = root.path().join(CONTENT_DIRECTORY).join(&digest);
        if hard {
            std::fs::hard_link(&source, &linked).unwrap();
        } else {
            std::os::unix::fs::symlink(&source, &linked).unwrap();
        }
        assert!(matches!(
            recover_pending_cleanup(&database, &store, "selected"),
            Err(MaintenanceFailure::Artifact(_))
        ));
        assert_eq!(std::fs::read(source).unwrap(), b"bytes");
        assert!(std::fs::symlink_metadata(linked).is_ok());
        assert_eq!(
            receipt(&database, "selected", "receipt").unwrap().unwrap().stage,
            ReceiptStage::DatabaseApplied
        );
    }
}
