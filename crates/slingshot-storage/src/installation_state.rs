//! Reading and replacing the one installation record, safely.
//!
//! Two properties matter more than the code that provides them.
//!
//! The record is replaced atomically: written to a temporary file beside it,
//! synchronized, renamed over it, and the directory synchronized. A reader
//! therefore sees either the whole old record or the whole new one, never a
//! half-written one - which is what lets the identity and the target ledger
//! live in one record and share one recovery story.
//!
//! And the file is validated through the handle it was opened with, not by
//! looking at the path and then opening it. Checking a path and reopening it is
//! two different files whenever anything changes in between, and this file
//! decides whether a daemon may keep an identity that remote subscriptions
//! depend on.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use slingshot_domain::installation::{InstallationFailure, InstallationRecord};

/// Name of the record inside the state root.
pub const RECORD_FILE_NAME: &str = "installation-state.json";

/// Name of the lock that serializes first start and registration.
pub const LOCK_FILE_NAME: &str = "installation-state.lock";

/// Suffix a partly written record carries until it is published.
const STAGING_SUFFIX: &str = ".staging";

/// Reason the record could not be read or replaced.
#[derive(Debug, thiserror::Error)]
pub enum InstallationStateFailure {
    /// The record is not there.
    #[error("the installation record is not there")]
    Absent,
    /// The record could not be read as one.
    #[error("the installation record could not be read: {0}")]
    Unreadable(String),
    /// The record is not a regular file the current user owns.
    #[error("the installation record is a regular file this user owns")]
    NotAPlainFile,
    /// The record says something this build does not implement.
    #[error("the installation record is not one this build can act on: {0}")]
    Unsupported(InstallationFailure),
    /// The filesystem refused something.
    #[error("the filesystem refused: {0}")]
    FilesystemRefused(String),
}

/// One state root, and the record inside it.
#[derive(Debug, Clone)]
pub struct InstallationState {
    /// Directory the record lives in.
    root: PathBuf,
}

impl InstallationState {
    /// Returns the state for the root at `root`.
    #[must_use]
    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Returns where the record lives.
    #[must_use]
    pub fn record_path(&self) -> PathBuf {
        self.root.join(RECORD_FILE_NAME)
    }

    /// Returns where the lock lives.
    ///
    /// One lock for the whole installation, independent of every per-target
    /// namespace lock, because first start and target registration are
    /// decisions about the installation rather than about one target.
    #[must_use]
    pub fn lock_path(&self) -> PathBuf {
        self.root.join(LOCK_FILE_NAME)
    }

    /// Returns whether anything at all exists under the state root.
    ///
    /// The question creating an identity is allowed to ask. A missing record is
    /// not enough: a missing record beside existing target state is exactly
    /// where inventing a replacement would strand live subscriptions.
    #[must_use]
    pub fn state_root_occupied(&self) -> bool {
        std::fs::read_dir(&self.root)
            .map(|entries| entries.flatten().next().is_some())
            .unwrap_or(false)
    }

    /// Reads the record.
    ///
    /// Validated through one open handle: the file is opened, its metadata read
    /// from that handle, and its bytes read from the same handle. Checking a
    /// path and then opening it would be checking one file and reading another
    /// whenever anything changed in between.
    ///
    /// # Errors
    ///
    /// Returns [`InstallationStateFailure`] naming what was wrong, and reads
    /// nothing further once anything is.
    pub fn read(&self) -> Result<InstallationRecord, InstallationStateFailure> {
        let path = self.record_path();
        let file = match std::fs::File::open(&path) {
            Ok(file) => file,
            Err(failure) if failure.kind() == std::io::ErrorKind::NotFound => {
                return Err(InstallationStateFailure::Absent);
            }
            Err(failure) => {
                return Err(InstallationStateFailure::FilesystemRefused(failure.to_string()));
            }
        };
        let metadata = file
            .metadata()
            .map_err(|failure| InstallationStateFailure::FilesystemRefused(failure.to_string()))?;
        if !metadata.is_file() {
            return Err(InstallationStateFailure::NotAPlainFile);
        }
        require_current_user_only(&metadata)?;
        let text = std::io::read_to_string(file)
            .map_err(|failure| InstallationStateFailure::Unreadable(failure.to_string()))?;
        let record: InstallationRecord = serde_json::from_str(&text)
            .map_err(|failure| InstallationStateFailure::Unreadable(failure.to_string()))?;
        record.require_supported().map_err(InstallationStateFailure::Unsupported)?;
        Ok(record)
    }

    /// Replaces the record with `record`, atomically.
    ///
    /// Written beside the record, synchronized, renamed over it, and the
    /// directory synchronized. A reader sees the whole old record or the whole
    /// new one.
    ///
    /// # Errors
    ///
    /// Returns [`InstallationStateFailure::FilesystemRefused`] when any step
    /// refuses, leaving the published record as it was.
    pub fn replace(&self, record: &InstallationRecord) -> Result<(), InstallationStateFailure> {
        let refused = |failure: std::io::Error| {
            InstallationStateFailure::FilesystemRefused(failure.to_string())
        };
        let written = serde_json::to_string(record)
            .map_err(|failure| InstallationStateFailure::Unreadable(failure.to_string()))?;
        let staging = self.root.join(format!("{RECORD_FILE_NAME}{STAGING_SUFFIX}"));
        let mut file = create_private(&staging)?;
        file.write_all(written.as_bytes()).map_err(refused)?;
        file.sync_all().map_err(refused)?;
        drop(file);
        std::fs::rename(&staging, self.record_path()).map_err(refused)?;
        synchronize_directory(&self.root)
    }
}

/// Requires a file to be reachable by its owner alone.
#[cfg(unix)]
fn require_current_user_only(metadata: &std::fs::Metadata) -> Result<(), InstallationStateFailure> {
    use std::os::unix::fs::MetadataExt as _;
    use std::os::unix::fs::PermissionsExt as _;

    /// Permission bits anyone but the owner would need.
    const OTHERS: u32 = 0o077;

    if metadata.uid() != nix_user() || metadata.permissions().mode() & OTHERS != 0 {
        return Err(InstallationStateFailure::NotAPlainFile);
    }
    Ok(())
}

/// Requires a file to be reachable by its owner alone.
#[cfg(not(unix))]
fn require_current_user_only(
    _metadata: &std::fs::Metadata,
) -> Result<(), InstallationStateFailure> {
    Ok(())
}

/// Returns the identifier of the user this process runs as.
#[cfg(unix)]
fn nix_user() -> u32 {
    uzers::get_current_uid()
}

/// Creates one file reachable by its owner alone.
#[cfg(unix)]
fn create_private(path: &Path) -> Result<std::fs::File, InstallationStateFailure> {
    use std::os::unix::fs::OpenOptionsExt as _;

    /// Permission bits a private file carries.
    const OWNER_ONLY: u32 = 0o600;

    std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(OWNER_ONLY)
        .open(path)
        .map_err(|failure| InstallationStateFailure::FilesystemRefused(failure.to_string()))
}

/// Creates one file reachable by its owner alone.
#[cfg(not(unix))]
fn create_private(path: &Path) -> Result<std::fs::File, InstallationStateFailure> {
    std::fs::File::create(path)
        .map_err(|failure| InstallationStateFailure::FilesystemRefused(failure.to_string()))
}

/// Synchronizes one directory so a rename is durable.
fn synchronize_directory(root: &Path) -> Result<(), InstallationStateFailure> {
    let directory = std::fs::File::open(root)
        .map_err(|failure| InstallationStateFailure::FilesystemRefused(failure.to_string()))?;
    // A directory sync is not supported everywhere, and where it is not the
    // rename is already durable. Refusing there would refuse a correct write.
    match directory.sync_all() {
        Ok(()) => Ok(()),
        Err(failure) if failure.kind() == std::io::ErrorKind::InvalidInput => Ok(()),
        Err(failure) => Err(InstallationStateFailure::FilesystemRefused(failure.to_string())),
    }
}
