//! Probe for the temporary-files capability.
//!
//! Requires an isolated directory that is removed when its handle drops, a
//! named temporary file that can be persisted atomically, and a path that never
//! collides with a second handle.

use std::io::Write;

use tempfile::{NamedTempFile, tempdir};

#[test]
fn a_temporary_root_isolates_and_removes_itself() {
    let removed;
    {
        let directory = tempdir().expect("a temporary directory is created");
        removed = directory.path().to_path_buf();
        assert!(removed.is_dir());
        let second = tempdir().expect("a second temporary directory is created");
        assert_ne!(second.path(), removed, "two roots never collide");

        let mut file = NamedTempFile::new_in(&removed).expect("a named file is created");
        file.write_all(b"credential").expect("the file is written");
        let target = removed.join("credentials.json");
        file.persist(&target).expect("the file is persisted atomically");
        assert_eq!(std::fs::read(&target).expect("the persisted file reads"), b"credential");
    }
    assert!(!removed.exists(), "the temporary root is removed when its handle drops");
}
