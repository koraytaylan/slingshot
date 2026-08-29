//! Probe for the file-locks capability.
//!
//! Requires an exclusive operating-system lock that a second handle cannot take
//! while it is held, that the owner can release, and that leaves the lock file
//! in place, because the daemon keeps its owner lock for its whole lifetime and
//! never unlinks it.

use std::fs::File;

use fs4::FileExt;

#[test]
fn an_exclusive_lock_excludes_a_second_handle_until_it_is_released() {
    let directory = tempfile::tempdir().expect("a temporary directory is created");
    let path = directory.path().join("owner.lock");
    let owner = File::create(&path).expect("the lock file is created");
    FileExt::try_lock(&owner).expect("the first handle takes the lock");
    let contender =
        File::options().read(true).write(true).open(&path).expect("a second handle opens");
    assert!(FileExt::try_lock(&contender).is_err(), "the second handle must be excluded");
    FileExt::unlock(&owner).expect("the owner releases the lock");
    FileExt::try_lock(&contender).expect("the second handle takes the released lock");
    FileExt::unlock(&contender).expect("the second handle releases the lock");
    assert!(path.is_file(), "the lock file outlives the lock");
}
