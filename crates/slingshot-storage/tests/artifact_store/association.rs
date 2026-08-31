//! Binding one operation's slot to the content that fills it.
//!
//! The bytes and the claim on them are separate durable things. Content is
//! addressed by its digest and shared by everything that produced identical
//! bytes; an association is one operation's claim on one slot, inside one
//! partition. These prove the two stay separate and commit together.

use slingshot_domain::command_fingerprint::{CommandFingerprint, FingerprintInput};
use slingshot_storage::artifact_store::{ArtifactAssociations, ArtifactFailure};
use slingshot_storage::database::{OperationDatabase, RequiredSettings};
use slingshot_storage::operation_repository::{
    AdmissionOutcome, AdmissionRequest, OperationRepository,
};

use crate::fixtures::*;

/// Bytes one page occupies, from the runtime contract.
const PAGE_BYTES: u64 = 4096;

/// Pages the database may reach, from the runtime contract.
const DATABASE_PAGES: u64 = 262_144;

/// Milliseconds a busy connection waits, from the runtime contract.
const BUSY_TIMEOUT: u64 = 5000;

/// Returns the settings every connection is held to.
fn settings() -> RequiredSettings {
    RequiredSettings {
        page_bytes: PAGE_BYTES,
        database_pages: DATABASE_PAGES,
        busy_timeout_milliseconds: BUSY_TIMEOUT,
    }
}

/// Returns one migrated database with the fixture operation already admitted.
///
/// The operation is admitted through the repository that owns admission rather
/// than seeded with a hand-written row, so an association is proved against the
/// rows the daemon actually writes. The file is then reopened, because these
/// are two collaborators over one database rather than one over the other.
fn database(directory: &tempfile::TempDir, digest: &str, operation: &str) -> OperationDatabase {
    let path = directory.path().join("operations.sqlite3");
    let repository =
        OperationRepository::new(OperationDatabase::open(&path, settings()).expect("a database"));
    let canonical = "{\"paths\":[\"/content\"]}";
    let asked = AdmissionRequest {
        author_target_identity: format!("opaque-identity-behind-{digest}"),
        author_target_identity_digest: digest.to_owned(),
        caller_identity: None,
        canonical_command: canonical.to_owned(),
        command_fingerprint: CommandFingerprint::derive(&FingerprintInput {
            author_target_identity_digest: digest.to_owned(),
            canonical_command: canonical.to_owned(),
            command_wire_name: "query_paths".to_owned(),
            command_semantic_contract_version: "1".to_owned(),
            selected_environment_revision: "revision-1".to_owned(),
        })
        .expect("a derivable fingerprint"),
        command_wire_name: "query_paths".to_owned(),
        daemon_runtime_contract_digest: "c".repeat(DIGEST_CHARACTERS),
        installation_identifier: installation(),
        operation_identifier: operation.to_owned(),
        selected_environment_revision: "revision-1".to_owned(),
        workflow_correlation_identifier: None,
    };
    let outcome = repository.admit(&asked, NOW).expect("an admission");
    assert!(matches!(outcome, AdmissionOutcome::Admitted(_)), "the fixture operation is admitted");
    drop(repository);
    OperationDatabase::open(&path, settings()).expect("a reopened database")
}

#[test]
fn an_association_reads_back_as_the_artifact_it_named() {
    let digest = partition(FIRST_PRINCIPAL);
    let operations = tempfile::tempdir().expect("a directory");
    let held = database(&operations, &digest, "operation-1");
    let associations = ArtifactAssociations::new(&held);
    let (_directory, store) = store();
    let metadata = store
        .install(
            &request(&digest, "operation-1", "content_package"),
            &mut content("streamed").as_slice(),
        )
        .expect("an installation");

    assert!(
        associations.read(&digest, "operation-1", "content_package").expect("a read").is_none(),
        "an operation that has produced nothing has no association"
    );
    associations.associate(&digest, "operation-1", &metadata, NOW).expect("an association");
    let read_back = associations
        .read(&digest, "operation-1", "content_package")
        .expect("a read")
        .expect("an association");
    assert_eq!(read_back.artifact_identifier, metadata.artifact_identifier);
    assert_eq!(read_back.byte_length, metadata.byte_length, "the exact length, not a rounded one");
    assert_eq!(read_back.content_digest, metadata.content_digest);
    assert_eq!(read_back.media_type, metadata.media_type);
    assert!(
        associations.read(&digest, "operation-1", "structured_result").expect("a read").is_none(),
        "and another slot of the same operation holds nothing"
    );
}

#[test]
fn an_association_never_names_content_the_store_has_no_record_of() {
    let digest = partition(FIRST_PRINCIPAL);
    let operations = tempfile::tempdir().expect("a directory");
    let held = database(&operations, &digest, "operation-1");
    let associations = ArtifactAssociations::new(&held);
    let (_directory, store) = store();
    let metadata = store
        .install(
            &request(&digest, "operation-1", "content_package"),
            &mut content("one-octet").as_slice(),
        )
        .expect("an installation");
    associations.associate(&digest, "operation-1", &metadata, NOW).expect("an association");

    let elsewhere = partition(SECOND_PRINCIPAL);
    let refused = associations.associate(&elsewhere, "operation-1", &metadata, NOW);
    assert!(
        matches!(refused, Err(ArtifactFailure::FilesystemRefused(_))),
        "a partition with no such operation cannot claim the slot: {refused:?}"
    );
    assert!(
        associations.read(&elsewhere, "operation-1", "content_package").expect("a read").is_none(),
        "and nothing was written"
    );
}

#[test]
fn identical_content_from_two_operations_records_one_blob() {
    let digest = partition(FIRST_PRINCIPAL);
    let operations = tempfile::tempdir().expect("a directory");
    let held = database(&operations, &digest, "operation-1");
    let associations = ArtifactAssociations::new(&held);
    let (_directory, store) = store();
    let first = store
        .install(
            &request(&digest, "operation-1", "content_package"),
            &mut content("one-octet").as_slice(),
        )
        .expect("an installation");
    let second = store
        .install(
            &request(&digest, "operation-1", "structured_result"),
            &mut content("duplicate-of-one-octet").as_slice(),
        )
        .expect("another slot holding the same bytes");
    assert_eq!(first.content_digest, second.content_digest);

    associations.associate(&digest, "operation-1", &first, NOW).expect("an association");
    associations.associate(&digest, "operation-1", &second, NOW).expect("another association");
    let blobs: i64 = held
        .connection()
        .query_row("SELECT COUNT(*) FROM artifact_blob", [], |row| row.get(0))
        .expect("a count");
    assert_eq!(blobs, 1, "one digest is one blob, however many slots point at it");
}
