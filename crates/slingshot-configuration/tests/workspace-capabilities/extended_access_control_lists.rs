//! Probe for the extended access-control-list capability.
//!
//! Requires reading the extended access-control list from an already-open file
//! descriptor rather than by path, so a credential file is checked through the
//! same object it is later read from.

use std::fs::File;

use xattr::FileExt;

/// Extended attribute that carries the extended access-control list.
const EXTENDED_LIST_ATTRIBUTE: &str = "com.apple.system.Security";

#[test]
fn a_credential_descriptor_reports_its_extended_access_control_evidence() {
    let directory = tempfile::tempdir().expect("a temporary directory is created");
    let file =
        File::create(directory.path().join("credentials.json")).expect("the credential is created");

    // macOS 26 intentionally rejects `getxattr` for every `com.apple.system.*`
    // name, including absent names.  The production row therefore uses the
    // list operation and only tests for presence of the ACL attribute.
    let names: Vec<String> = file
        .list_xattr()
        .expect("the descriptor lists its attributes")
        .map(|name| name.to_string_lossy().into_owned())
        .collect();
    assert!(!names.iter().any(|name| name == EXTENDED_LIST_ATTRIBUTE), "{names:?}");
}
