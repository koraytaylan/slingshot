//! Probe for the POSIX access-control-list capability.
//!
//! Requires reading the access-control list from an already-open file
//! descriptor rather than by path, and seeing every entry a widened credential
//! file would carry: the owner entry, a named user entry, the mask that bounds
//! it, and the other entry.

use std::fs::File;

use xattr::FileExt;

/// Extended attribute that carries the access access-control list.
const ACCESS_LIST_ATTRIBUTE: &str = "system.posix_acl_access";

/// Version every POSIX access-control-list record declares.
const ACCESS_LIST_VERSION: u32 = 2;

/// Tag of the entry that describes the owning user.
const OWNER_TAG: u16 = 0x01;

/// Tag of an entry that names another user.
const NAMED_USER_TAG: u16 = 0x02;

/// Tag of the entry that describes the owning group.
const OWNING_GROUP_TAG: u16 = 0x04;

/// Tag of the entry that bounds every named and group entry.
const MASK_TAG: u16 = 0x10;

/// Tag of the entry that describes every other user.
const OTHER_TAG: u16 = 0x20;

/// Identity recorded for an entry that names no principal.
const UNNAMED_IDENTITY: u32 = u32::MAX;

/// Permission bits of a readable and writable entry.
const READ_AND_WRITE: u16 = 6;

/// Permission bits of a readable entry.
const READ_ONLY: u16 = 4;

/// Permission bits of an entry that grants nothing.
const NO_ACCESS: u16 = 0;

/// Identity of the named user the probe widens the file to.
const NAMED_USER_IDENTITY: u32 = 1234;

/// Number of bytes one access-control-list entry occupies.
const ENTRY_LENGTH: usize = 8;

/// One decoded access-control-list entry.
#[derive(Debug, PartialEq, Eq)]
struct AccessEntry {
    tag: u16,
    permissions: u16,
    identity: u32,
}

/// Renders one access-control list into its stored bytes.
fn encode(entries: &[AccessEntry]) -> Vec<u8> {
    let mut bytes = ACCESS_LIST_VERSION.to_le_bytes().to_vec();
    for entry in entries {
        bytes.extend_from_slice(&entry.tag.to_le_bytes());
        bytes.extend_from_slice(&entry.permissions.to_le_bytes());
        bytes.extend_from_slice(&entry.identity.to_le_bytes());
    }
    bytes
}

/// Reads one stored access-control list back into its entries.
fn decode(bytes: &[u8]) -> Vec<AccessEntry> {
    let version = u32::from_le_bytes(bytes[..4].try_into().expect("the record has a version"));
    assert_eq!(version, ACCESS_LIST_VERSION);
    bytes[4..]
        .as_chunks::<ENTRY_LENGTH>()
        .0
        .iter()
        .map(|chunk| AccessEntry {
            tag: u16::from_le_bytes(chunk[0..2].try_into().expect("the entry has a tag")),
            permissions: u16::from_le_bytes(
                chunk[2..4].try_into().expect("the entry has permissions"),
            ),
            identity: u32::from_le_bytes(
                chunk[4..8].try_into().expect("the entry has an identity"),
            ),
        })
        .collect()
}

#[test]
fn a_widened_credential_file_shows_its_named_entry_and_mask_through_its_descriptor() {
    let directory = tempfile::tempdir().expect("a temporary directory is created");
    let path = directory.path().join("credentials.json");
    let file = File::create(&path).expect("the credential is created");

    let written = vec![
        AccessEntry { tag: OWNER_TAG, permissions: READ_AND_WRITE, identity: UNNAMED_IDENTITY },
        AccessEntry { tag: NAMED_USER_TAG, permissions: READ_ONLY, identity: NAMED_USER_IDENTITY },
        AccessEntry { tag: OWNING_GROUP_TAG, permissions: READ_ONLY, identity: UNNAMED_IDENTITY },
        AccessEntry { tag: MASK_TAG, permissions: READ_ONLY, identity: UNNAMED_IDENTITY },
        AccessEntry { tag: OTHER_TAG, permissions: NO_ACCESS, identity: UNNAMED_IDENTITY },
    ];
    file.set_xattr(ACCESS_LIST_ATTRIBUTE, &encode(&written))
        .expect("the access-control list is stored");

    let stored = file
        .get_xattr(ACCESS_LIST_ATTRIBUTE)
        .expect("the descriptor is readable")
        .expect("the access-control list is present");
    assert_eq!(decode(&stored), written);

    let names: Vec<String> = file
        .list_xattr()
        .expect("the descriptor lists its attributes")
        .map(|name| name.to_string_lossy().into_owned())
        .collect();
    assert!(names.iter().any(|name| name == ACCESS_LIST_ATTRIBUTE), "{names:?}");

    let absent =
        File::create(directory.path().join("plain.json")).expect("a plain file is created");
    assert_eq!(
        absent.get_xattr(ACCESS_LIST_ATTRIBUTE).expect("the descriptor is readable"),
        None,
        "a file with no widened list reports none"
    );
}
