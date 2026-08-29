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

    let evidence = file.get_xattr(EXTENDED_LIST_ATTRIBUTE).expect("the descriptor is readable");
    assert_eq!(evidence, None, "a freshly created credential carries no extended list");

    let names: Vec<String> = file
        .list_xattr()
        .expect("the descriptor lists its attributes")
        .map(|name| name.to_string_lossy().into_owned())
        .collect();
    assert!(!names.iter().any(|name| name == EXTENDED_LIST_ATTRIBUTE), "{names:?}");
}
