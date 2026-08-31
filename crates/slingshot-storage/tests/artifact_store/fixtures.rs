//! The values and helpers every part of this suite is built from.

use serde_json::Value;
use slingshot_domain::installation::InstallationIdentifier;
pub use slingshot_storage::artifact_store::DIGEST_CHARACTERS;
use slingshot_storage::artifact_store::{ArtifactStore, InstallationRequest};

/// Content vectors this suite reads.
pub const CONTENTS: &str = include_str!("../fixtures/artifacts/contents.jsonl");

/// Identifier vectors this suite reads.
pub const IDENTIFIERS: &str = include_str!("../fixtures/artifacts/identifiers.jsonl");

/// Two-character pairs in a sixty-four-character hexadecimal value.
pub const DIGEST_PAIRS: usize = 32;

/// One instant, for a test that does not care which.
pub const NOW: u64 = 1_700_000_000_000;

/// Bytes the largest inline result occupies, from the runtime contract.
pub const INLINE_RESULT_BYTES: usize = 2048;

/// Bytes one read of an installing stream moves.
pub const TRANSFER_BYTES: usize = 65_536;

/// Returns one row's string member.
pub fn text<'row>(row: &'row Value, member: &str) -> &'row str {
    row[member].as_str().unwrap_or_else(|| panic!("{member} is a string in {row}"))
}

/// Returns every row of one fixture.
pub fn rows(fixture: &str) -> Vec<Value> {
    fixture
        .lines()
        .map(|line| serde_json::from_str(line).expect("every fixture line is one object"))
        .collect()
}

/// Returns the bytes one content fixture holds.
///
/// Named with or without the extension, because the fixture rows carry the file
/// name and the tests that reach for one directly carry the vector's name.
pub fn content(name: &str) -> Vec<u8> {
    let directory =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/artifacts");
    let file = format!("{}.bin", name.trim_end_matches(".bin"));
    std::fs::read(directory.join(&file))
        .unwrap_or_else(|failure| panic!("the {file} fixture reads: {failure}"))
}

/// Returns the installation identifier every fixture is installed under.
pub fn installation() -> InstallationIdentifier {
    InstallationIdentifier::parse(&"a1".repeat(DIGEST_PAIRS)).expect("a legal identifier")
}

/// Returns the digest one principal's author target has.
pub fn partition(principal: &str) -> String {
    principal.repeat(DIGEST_PAIRS)
}

/// The first of the two partitions a same-deployment fixture uses.
pub const FIRST_PRINCIPAL: &str = "1d";

/// The second of those two partitions.
pub const SECOND_PRINCIPAL: &str = "2d";

/// Returns one installation request.
pub fn request(digest: &str, operation: &str, slot: &str) -> InstallationRequest {
    InstallationRequest {
        artifact_slot: slot.to_owned(),
        author_target_identity_digest: digest.to_owned(),
        descriptor: Some("a package the remote produced".to_owned()),
        installation_identifier: installation(),
        media_type: "application/zip".to_owned(),
        operation_identifier: operation.to_owned(),
    }
}

/// Returns a directory and one store rooted inside it.
pub fn store() -> (tempfile::TempDir, ArtifactStore) {
    let directory = tempfile::tempdir().expect("a directory");
    let store = ArtifactStore::open(directory.path()).expect("a store");
    (directory, store)
}

/// Bytes the largest canonical structured result occupies, from the contract.
pub const CANONICAL_STRUCTURED_RESULT_BYTES: u64 = 1_048_576;

/// Requires one file to be reachable by its owner alone.
#[cfg(unix)]
pub fn require_owner_only(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt as _;

    /// Permission bits anyone but the owner would need.
    const OTHERS: u32 = 0o077;

    let metadata = std::fs::metadata(path).expect("the file reads");
    assert_eq!(
        metadata.permissions().mode() & OTHERS,
        0,
        "installed content carries no permission for anyone else"
    );
}

/// Requires one file to be reachable by its owner alone.
#[cfg(not(unix))]
pub fn require_owner_only(_path: &std::path::Path) {}

/// Widens one file's permissions so the store should refuse to read it.
#[cfg(unix)]
pub fn make_reachable_by_others(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt as _;

    /// Owner read and write, and read for everyone else.
    const WIDE_OPEN: u32 = 0o644;

    std::fs::set_permissions(path, std::fs::Permissions::from_mode(WIDE_OPEN))
        .expect("the permissions change");
}

/// Widens one file's permissions so the store should refuse to read it.
#[cfg(not(unix))]
pub fn make_reachable_by_others(_path: &std::path::Path) {}

/// Bytes one read of a reading test moves.
///
/// Deliberately smaller than the store's own transfer, so a test reading a
/// multi-transfer fixture crosses the store's boundary somewhere other than on
/// its own.
pub const READ_BYTES: usize = 4096;

/// The divisor one truncation test halves a length by.
pub const HALF: u64 = 2;
