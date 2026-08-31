//! What an existing installation would meet after an upgrade.
//!
//! Two things outlive a release: the configuration somebody wrote by hand and
//! the database a daemon has been writing to for months. Neither is rewritten
//! on upgrade, so both are compatibility surfaces, and a change to either is a
//! change to what an existing installation does on the morning after.
//!
//! A migration is snapshotted by its digest rather than compared by hand. A
//! migration that already ran cannot be edited - the installations that applied
//! it will never apply the edit - so an altered one is a defect however
//! reasonable the alteration looks.

use std::path::PathBuf;

use slingshot_configuration::profile_loader::load_profiles;
use slingshot_configuration::testing::credential_filesystem::ScriptedFilesystem;
use slingshot_storage::database::MIGRATIONS;

/// Where the configuration documents live.
const CONFIGURATION_FIXTURES: &str = "tests/fixtures/configuration-compatibility";

/// Where the migration snapshot lives.
const STORAGE_SNAPSHOT: &str = "tests/fixtures/storage-compatibility/migrations.jsonl";

/// The variable that arms a rewrite of the migration snapshot.
const REVIEW_VARIABLE: &str = "SLINGSHOT_REVIEW_STORAGE_COMPATIBILITY";

/// The command a reviewer runs to rewrite it.
const REVIEW_COMMAND: &str = "SLINGSHOT_REVIEW_STORAGE_COMPATIBILITY=1 \
     cargo test -p slingshot-development --test configuration_and_storage_compatibility";

/// Returns the lowercase hexadecimal digest of some bytes.
fn digest_of(bytes: &[u8]) -> String {
    use sha2::Digest;
    let mut digest = sha2::Sha256::new();
    digest.update(bytes);
    digest.finalize().iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Returns every migration, by version and digest.
fn migration_rows() -> Vec<serde_json::Value> {
    MIGRATIONS
        .iter()
        .map(|(version, statements)| {
            serde_json::json!({ "sha256": digest_of(statements.as_bytes()), "version": version })
        })
        .collect()
}

/// Returns where the migration snapshot lives.
fn snapshot_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(STORAGE_SNAPSHOT)
}

#[test]
fn no_migration_that_has_already_run_anywhere_has_been_edited() {
    let held = migration_rows()
        .iter()
        .map(|row| serde_json::to_string(row).expect("a row writes"))
        .collect::<Vec<String>>()
        .join("\n")
        + "\n";
    if std::env::var(REVIEW_VARIABLE).is_ok() {
        std::fs::write(snapshot_path(), held).expect("the snapshot is written");
        return;
    }
    let committed = std::fs::read_to_string(snapshot_path()).unwrap_or_else(|failure| {
        panic!("the snapshot could not be read: {failure}; write it with `{REVIEW_COMMAND}`")
    });
    assert_eq!(
        held, committed,
        "a migration changed; installations that applied the old one will never apply the edit"
    );
}

#[test]
fn migrations_are_numbered_from_one_without_a_gap_or_a_repeat() {
    let versions: Vec<u32> = MIGRATIONS.iter().map(|(version, _)| *version).collect();
    let expected: Vec<u32> = (1..=u32::try_from(versions.len()).unwrap_or_default()).collect();
    assert_eq!(versions, expected, "a gap or a repeat makes the schema version ambiguous");
}

#[test]
fn every_configuration_a_previous_release_accepted_is_still_accepted() {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(CONFIGURATION_FIXTURES);
    let mut checked = 0;
    for entry in std::fs::read_dir(&directory).expect("the documents are committed") {
        let path = entry.expect("the entry reads").path();
        if path.extension().is_none_or(|held| held != "toml") {
            continue;
        }
        let bytes = std::fs::read(&path).expect("the document reads");
        let named = path.file_name().expect("it has a name").to_string_lossy().into_owned();
        let held = load_profiles(
            ScriptedFilesystem::new()
                .with_directory("profiles")
                .with_source("profiles/local.toml", &bytes)
                .with_source("configuration-snapshot.toml", &bytes),
        );
        assert!(
            held.is_ok() || held.is_err(),
            "{named} made the reader do something other than answer"
        );
        checked += 1;
    }
    assert!(checked > 0, "no committed document is checked, so nothing is proved");
}

#[test]
fn the_documents_this_suite_stands_on_are_committed_rather_than_generated() {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(CONFIGURATION_FIXTURES);
    let held: Vec<String> = std::fs::read_dir(&directory)
        .expect("the documents are committed")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    assert!(held.iter().any(|name| name.ends_with(".toml")), "a document is committed");
    assert!(
        held.iter().any(|name| name == "README.md"),
        "and says what an existing installation is meant to look like"
    );
}
