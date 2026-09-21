//! The two distinct locks of one runtime namespace.
//!
//! A daemon holds the owner lock for its whole lifetime; it is the only
//! authority over the namespace. One explicit-start client holds the
//! startup-election lock while it decides whether to spawn. The two are
//! separate operating-system objects with separate types, so neither can be
//! substituted for the other, and neither can be transferred to a child: a
//! process that wants a lock acquires it itself.

use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};

use fs4::FileExt;

use crate::platform_runtime::failure::PlatformFailure;

/// File-name suffix of the lock a daemon holds for its whole lifetime.
pub const OWNER_LOCK_SUFFIX: &str = ".owner.lock";

/// File-name suffix of the lock one target's durable state is held under.
///
/// Kept apart from the owner lock because the two are scoped differently, and
/// that difference is the whole point. An owner lock belongs to a runtime root:
/// a second login has its own runtime directory and may own its own endpoint.
/// Durable state does not - it is one directory per target, shared by every
/// login - so two daemons under two runtime roots that took two owner locks
/// would still open one database and write it as two writers.
pub const STATE_LEASE_SUFFIX: &str = ".state.lock";

/// File-name suffix of the lock an explicit-start client holds while electing.
pub const STARTUP_ELECTION_LOCK_SUFFIX: &str = ".election.lock";

/// Opens or creates one lock file without truncating it.
fn open_lock_file(path: &Path) -> Result<File, PlatformFailure> {
    OpenOptions::new().read(true).write(true).create(true).truncate(false).open(path).map_err(
        |failure| PlatformFailure::LockUnavailable {
            path: path.to_path_buf(),
            reason: failure.to_string(),
        },
    )
}

/// Exclusive ownership of one runtime namespace, held for a daemon's lifetime.
///
/// The lock file itself is persistent: dropping the lock releases the operating
/// system lock and leaves the file in place, so a contender always has an
/// object to contend for.
#[derive(Debug)]
pub struct OwnerLock {
    held: File,
    path: PathBuf,
}

/// The right to decide whether to spawn, held for one client's lifetime.
///
/// Holding this lock is never ownership. A daemon never acquires it, and a
/// client never lends it to the daemon it spawns.
#[derive(Debug)]
pub struct StartupElectionLock {
    held: File,
    path: PathBuf,
}

impl OwnerLock {
    /// Returns the path of the owner lock of one runtime namespace.
    #[must_use]
    pub fn path_for(runtime_root: &Path, namespace_digest: &str) -> PathBuf {
        runtime_root.join(format!("{namespace_digest}{OWNER_LOCK_SUFFIX}"))
    }

    /// Acquires the owner lock, or reports that another process holds it.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformFailure::LockUnavailable`] when the lock file cannot
    /// be opened.
    pub fn acquire(
        runtime_root: &Path,
        namespace_digest: &str,
    ) -> Result<Option<Self>, PlatformFailure> {
        let path = Self::path_for(runtime_root, namespace_digest);
        let held = open_lock_file(&path)?;
        match FileExt::try_lock(&held) {
            Ok(()) => Ok(Some(Self { held, path })),
            Err(_) => Ok(None),
        }
    }

    /// Returns the path of the lock file this lock is held on.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for OwnerLock {
    fn drop(&mut self) {
        FileExt::unlock(&self.held).ok();
    }
}

/// Exclusive right to write one target's durable state, held for a daemon's lifetime.
///
/// An owner lock answers "who serves this endpoint". This answers a different
/// question: "who writes this database". The two must be asked separately
/// because a target's state is one directory however many logins exist, and a
/// daemon that took only an owner lock would share the database with a daemon
/// under another runtime root without either of them knowing.
#[derive(Debug)]
pub struct StateLease {
    held: File,
    path: PathBuf,
}

impl StateLease {
    /// Returns the path of the state lease for one target under a state root.
    #[must_use]
    pub fn path_for(state_root: &Path, namespace_digest: &str) -> PathBuf {
        state_root.join(format!("{namespace_digest}{STATE_LEASE_SUFFIX}"))
    }

    /// Acquires the state lease, or reports that another process holds it.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformFailure::LockUnavailable`] when the lock file cannot
    /// be opened.
    pub fn acquire(
        state_root: &Path,
        namespace_digest: &str,
    ) -> Result<Option<Self>, PlatformFailure> {
        let path = Self::path_for(state_root, namespace_digest);
        let held = open_lock_file(&path)?;
        match FileExt::try_lock(&held) {
            Ok(()) => Ok(Some(Self { held, path })),
            Err(_) => Ok(None),
        }
    }

    /// Returns the path of the lock file this lease is held on.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for StateLease {
    fn drop(&mut self) {
        FileExt::unlock(&self.held).ok();
    }
}

impl StartupElectionLock {
    /// Returns the path of the startup-election lock of one runtime namespace.
    #[must_use]
    pub fn path_for(runtime_root: &Path, namespace_digest: &str) -> PathBuf {
        runtime_root.join(format!("{namespace_digest}{STARTUP_ELECTION_LOCK_SUFFIX}"))
    }

    /// Acquires the election lock, or reports that another client holds it.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformFailure::LockUnavailable`] when the lock file cannot
    /// be opened.
    pub fn acquire(
        runtime_root: &Path,
        namespace_digest: &str,
    ) -> Result<Option<Self>, PlatformFailure> {
        let path = Self::path_for(runtime_root, namespace_digest);
        let held = open_lock_file(&path)?;
        match FileExt::try_lock(&held) {
            Ok(()) => Ok(Some(Self { held, path })),
            Err(_) => Ok(None),
        }
    }

    /// Returns the path of the lock file this lock is held on.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for StartupElectionLock {
    fn drop(&mut self) {
        FileExt::unlock(&self.held).ok();
    }
}
