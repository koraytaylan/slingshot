//! Probe for the Windows file-identity capability.
//!
//! Requires opening a credential without traversing a reparse point, and
//! reading the reparse evidence, the link count, the volume serial number, and
//! the volume-scoped identifier from that same handle, all through a safe
//! interface.

use winsafe::{HFILE, co};

/// Opens the credential without traversing a reparse point.
fn open_without_traversing(name: &str) -> winsafe::guard::CloseHandleGuard<HFILE> {
    let (handle, _) = HFILE::CreateFile(
        name,
        co::GENERIC::READ,
        Some(co::FILE_SHARE::READ),
        None,
        co::DISPOSITION::OPEN_EXISTING,
        co::FILE_ATTRIBUTE::NORMAL,
        Some(co::FILE_FLAG::OPEN_REPARSE_POINT),
        None,
        None,
    )
    .expect("the credential opens without traversing a reparse point");
    handle
}

#[test]
fn a_credential_handle_reports_its_reparse_evidence_and_volume_scoped_identity() {
    let directory = tempfile::tempdir().expect("a temporary directory is created");
    let path = directory.path().join("credentials.json");
    std::fs::write(&path, b"{}").expect("the credential is created");
    let name = path.to_str().expect("the path is text");

    let handle = open_without_traversing(name);
    let information = handle.GetFileInformationByHandle().expect("the handle reports its identity");
    assert_ne!(information.dwVolumeSerialNumber, 0, "the object names its volume");
    assert_eq!(information.nNumberOfLinks, 1, "the credential is not hard-linked");
    assert_ne!(information.nFileIndex(), 0, "the object has an identity on its volume");
    assert!(
        !information.dwFileAttributes.has(co::FILE_ATTRIBUTE::REPARSE_POINT),
        "the credential is not a reparse point"
    );

    let second = open_without_traversing(name);
    let repeated =
        second.GetFileInformationByHandle().expect("the second handle reports its identity");
    assert_eq!(repeated.nFileIndex(), information.nFileIndex(), "the identity is stable");
    assert_eq!(repeated.dwVolumeSerialNumber, information.dwVolumeSerialNumber);
}
