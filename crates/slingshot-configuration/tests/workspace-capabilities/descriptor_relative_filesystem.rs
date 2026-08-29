//! Probe for the descriptor-relative filesystem capability.
//!
//! Requires opening a descendant relative to a directory descriptor without
//! following a link, reading identity from that same open object, and refusing
//! a link that would leave the directory, because a credential file is checked
//! and then read through one object rather than re-opened by path.

use std::fs::File;
use std::io::Write;
use std::os::unix::fs::symlink;

use rustix::fs::{Mode, OFlags, openat};

#[test]
fn a_descendant_opens_relative_to_a_descriptor_without_following_a_link() {
    let directory = tempfile::tempdir().expect("a temporary directory is created");
    let mut credential =
        File::create(directory.path().join("credentials.json")).expect("the credential is created");
    credential.write_all(b"{}").expect("the credential is written");
    symlink("/etc/passwd", directory.path().join("escape.json")).expect("the link is created");

    let root = rustix::fs::open(
        directory.path(),
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .expect("the directory descriptor opens");

    let opened = openat(
        &root,
        "credentials.json",
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .expect("the descendant opens relative to the descriptor");

    let identity = rustix::fs::fstat(&opened).expect("the open object reports its identity");
    assert_ne!(identity.st_ino, 0, "the object has an inode identity");
    assert_ne!(identity.st_dev, 0, "the object names its device");
    assert_eq!(identity.st_nlink, 1, "the credential is not hard-linked");

    let directory_identity = rustix::fs::fstat(&root).expect("the directory reports its identity");
    assert_eq!(directory_identity.st_dev, identity.st_dev, "both objects share one device");
    assert_ne!(directory_identity.st_ino, identity.st_ino);

    let refused = openat(
        &root,
        "escape.json",
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    );
    assert!(refused.is_err(), "a link must not be followed out of the directory");

    let followed = openat(&root, "escape.json", OFlags::RDONLY | OFlags::CLOEXEC, Mode::empty());
    assert!(followed.is_ok(), "the same descendant resolves when links are followed");
}
