//! Startup discards only abandoned private stages, not publication evidence.

use sha2::{Digest, Sha256};
use slingshot_domain::{
    installation::InstallationIdentifier, persistent_capacity::PersistentCapacityPolicy,
};
use slingshot_storage::{
    artifact_store::{ArtifactStore, CONTENT_DIRECTORY, InstallationRequest, STAGING_SUFFIX},
    database::{OperationDatabase, RequiredSettings},
    persistent_capacity::PersistentCapacityAccount,
};

#[test]
fn stage_recovery_preserves_publication_holds_and_addressed_content() {
    let root = tempfile::tempdir().unwrap();
    let database = OperationDatabase::open(
        &root.path().join("operations.sqlite3"),
        RequiredSettings {
            page_bytes: 4096,
            database_pages: 262144,
            busy_timeout_milliseconds: 5000,
        },
    )
    .unwrap();
    let store_root = root.path().join("artifacts");
    let store = ArtifactStore::open(&store_root).unwrap();
    let capacity = PersistentCapacityAccount::new(&database, PersistentCapacityPolicy::embedded());
    let request = InstallationRequest {
        artifact_slot: "result".into(),
        author_target_identity_digest: "target".into(),
        descriptor: None,
        installation_identifier: InstallationIdentifier::parse(&"a".repeat(64)).unwrap(),
        media_type: "application/json".into(),
        operation_identifier: "local".into(),
    };
    let bytes = b"{}";
    let digest: String = Sha256::digest(bytes).iter().map(|byte| format!("{byte:02x}")).collect();
    let reservation = capacity.reserve_artifact(Some(&digest), bytes.len() as u64).unwrap();
    let stage =
        store.stage_verified(&request, &mut &bytes[..], bytes.len() as u64, &digest).unwrap();
    let publication = capacity.retain_staged_publication(&stage, reservation, 1).unwrap();
    let metadata = stage.metadata().clone();
    // A process crash skips the stage's normal cleanup destructor. The finished
    // stage owns no open writer; forgetting it retains only its temporary name.
    std::mem::forget(stage);
    let published = store
        .stage_verified(&request, &mut &bytes[..], bytes.len() as u64, &digest)
        .unwrap()
        .publish()
        .unwrap();
    let content = store_root.join(CONTENT_DIRECTORY).join(&digest);
    assert_eq!(std::fs::read(&content).unwrap(), bytes);
    assert_eq!(store.recover_abandoned_stages().unwrap(), 1);
    assert_eq!(store.recover_abandoned_stages().unwrap(), 0);
    assert_eq!(std::fs::read(&content).unwrap(), bytes);
    assert_eq!(published, metadata);
    assert_eq!(capacity.pending_publications().unwrap(), 1);
    let recovered = capacity.reconstruct_publications().unwrap();
    assert_eq!(recovered.len(), 1);
    assert_eq!(recovered[0].publication_identifier, publication.identifier());
    assert_eq!(recovered[0].artifact_identifier, metadata.artifact_identifier);
    assert_eq!(recovered[0].content_digest, digest);
    assert_eq!(recovered[0].byte_length, bytes.len() as u64);
    assert_eq!(recovered[0].recorded_at_unix_milliseconds, 1);
    assert_eq!(format!("{:?}", recovered[0]), "PendingArtifactPublication([redacted])");
    assert_eq!(
        capacity.recover_publication(&metadata).unwrap().unwrap().identifier(),
        publication.identifier()
    );
    assert_eq!(capacity.usage().unwrap().committed_artifact_bytes, bytes.len() as u64);
    let duplicate_stage =
        store.stage_verified(&request, &mut &bytes[..], bytes.len() as u64, &digest).unwrap();
    let second = capacity.retain_staged_publication(&duplicate_stage, None, 2).unwrap();
    drop(duplicate_stage);
    let multiple = capacity.reconstruct_publications().unwrap();
    assert_eq!(multiple.len(), 2, "shared content collapsed distinct producer holds");
    assert!(multiple.iter().any(|held| held.publication_identifier == second.identifier()));
    assert!(capacity.recover_publication(&metadata).is_err(), "ambiguous producer was chosen");
    let mut too_small = PersistentCapacityPolicy::embedded();
    too_small.retained_operation_rows = 0;
    assert!(
        PersistentCapacityAccount::new(&database, too_small).reconstruct_publications().is_err()
    );
    assert_eq!(capacity.reconstruct_publications().unwrap(), multiple);
    let reopened = OperationDatabase::open_live(
        &root.path().join("operations.sqlite3"),
        RequiredSettings {
            page_bytes: 4096,
            database_pages: 262144,
            busy_timeout_milliseconds: 5000,
        },
    )
    .unwrap();
    assert_eq!(
        PersistentCapacityAccount::new(&reopened, PersistentCapacityPolicy::embedded())
            .reconstruct_publications()
            .unwrap(),
        multiple
    );
}

#[test]
#[cfg(unix)]
fn stage_recovery_refuses_links_and_unknown_stage_names() {
    for mode in ["symlink", "hard-link", "unknown-name"] {
        let root = tempfile::tempdir().unwrap();
        let store = ArtifactStore::open(root.path()).unwrap();
        let name = if mode == "unknown-name" {
            format!("unknown{STAGING_SUFFIX}")
        } else {
            format!("{}{STAGING_SUFFIX}", uuid::Uuid::new_v4())
        };
        let path = root.path().join(CONTENT_DIRECTORY).join(name);
        let source = root.path().join("retained");
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::write(&source, b"retained").unwrap();
        std::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o600)).unwrap();
        match mode {
            "symlink" => std::os::unix::fs::symlink(&source, &path).unwrap(),
            "hard-link" => std::fs::hard_link(&source, &path).unwrap(),
            _ => std::fs::write(&path, b"unknown").unwrap(),
        }
        assert!(store.recover_abandoned_stages().is_err());
        assert!(std::fs::symlink_metadata(&path).is_ok());
        assert_eq!(std::fs::read(&source).unwrap(), b"retained");
    }
}
