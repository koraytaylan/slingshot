//! Probe for the Windows security-identifier capability.
//!
//! Requires reading the owner security identifier and the discretionary
//! access-control list of an already-open credential handle, rendering an
//! identifier as text, and building the well-known identifiers a credential
//! check compares against, all through a safe interface.

use std::fs::File;

use windows_permissions::constants::{SeObjectType, SecurityInformation};
use windows_permissions::wrappers::{ConvertSidToStringSid, GetSecurityInfo};
use windows_permissions::{LocalBox, Sid};

/// Canonical text of the local system security identifier.
const LOCAL_SYSTEM_IDENTIFIER: &str = "S-1-5-18";

/// Canonical text of the built-in administrators security identifier.
const BUILTIN_ADMINISTRATORS_IDENTIFIER: &str = "S-1-5-32-544";

#[test]
fn a_credential_handle_reports_its_owner_and_discretionary_entries() {
    let directory = tempfile::tempdir().expect("a temporary directory is created");
    let path = directory.path().join("credentials.json");
    std::fs::write(&path, b"{}").expect("the credential is created");
    let handle = File::open(&path).expect("the credential opens");

    let descriptor = GetSecurityInfo(
        &handle,
        SeObjectType::SE_FILE_OBJECT,
        SecurityInformation::Owner | SecurityInformation::Dacl,
    )
    .expect("the handle reports its security descriptor");

    let owner = descriptor.owner().expect("the descriptor names an owner");
    let rendered = ConvertSidToStringSid(owner).expect("the identifier renders as text");
    assert!(rendered.to_string_lossy().starts_with("S-1-"), "{rendered:?}");

    let list = descriptor.dacl().expect("the descriptor carries a discretionary list");
    assert!(list.len() > 0, "the credential grants at least one entry");
    for index in 0..list.len() {
        let entry = list.get_ace(index).expect("every entry is readable");
        assert!(entry.sid().is_some(), "every entry names a principal");
    }

    let system: LocalBox<Sid> =
        LOCAL_SYSTEM_IDENTIFIER.parse().expect("the system identifier reads back");
    let administrators: LocalBox<Sid> = BUILTIN_ADMINISTRATORS_IDENTIFIER
        .parse()
        .expect("the administrators identifier reads back");
    assert_ne!(system.as_ref(), administrators.as_ref());
    assert_eq!(
        ConvertSidToStringSid(system.as_ref()).expect("the identifier renders").to_string_lossy(),
        LOCAL_SYSTEM_IDENTIFIER
    );
    assert_ne!(system.as_ref(), owner);
}
