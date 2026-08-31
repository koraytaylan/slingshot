//! Making sure two commands do not stage one artifact at once.
//!
//! Two processes writing one partial download produce a file that is neither of
//! them, and the failure looks like corruption rather than a race - which is
//! the worst way for it to look, because the natural response is to distrust
//! the artifact rather than the client.
//!
//! # The names are derived, not chosen
//!
//! The lock, the staging file, and the sidecar all sit beside the destination
//! and are named from the target, the revision, and the closed payload
//! identity. Derived names mean two invocations of the same fetch collide on
//! purpose, and two different fetches never collide by accident.

use crate::artifact_staging_metadata::StagedPayload;

/// The suffix a staging file carries.
pub const STAGING_SUFFIX: &str = ".slingshot-partial";

/// The suffix a sidecar carries.
pub const SIDECAR_SUFFIX: &str = ".slingshot-record";

/// The suffix a lock carries.
pub const LOCK_SUFFIX: &str = ".slingshot-lock";

/// The separator between the parts of a derived name.
const PART_SEPARATOR: char = '.';

/// Returns the stem every file of one transfer is named from.
///
/// The target, the revision, and the payload identity, in that order. Two
/// invocations of the same fetch derive the same stem and collide on purpose;
/// two different fetches derive different stems and never collide by accident.
#[must_use]
pub fn stem(
    author_target_identity_digest: &str,
    selected_environment_revision: &str,
    payload: &StagedPayload,
) -> String {
    let identity = match payload {
        StagedPayload::OperationArtifact { artifact_identifier, operation_identifier } => {
            format!("{operation_identifier}{PART_SEPARATOR}{artifact_identifier}")
        }
        StagedPayload::MaintenanceResult { maintenance_result_identifier } => {
            maintenance_result_identifier.clone()
        }
    };
    format!(
        "{author_target_identity_digest}{PART_SEPARATOR}{selected_environment_revision}\
         {PART_SEPARATOR}{identity}"
    )
}

/// The three files one transfer keeps beside its destination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagingNames {
    /// Where the exclusive lock lives.
    pub lock: std::path::PathBuf,
    /// Where the facts live.
    pub sidecar: std::path::PathBuf,
    /// Where the bytes accumulate.
    pub staging: std::path::PathBuf,
}

/// Returns where one transfer's three files live, beside `destination`.
///
/// The same directory, so the publication can be a rename rather than a copy: a
/// rename across filesystems is not atomic, and the atomicity is the whole
/// reason the staging file exists.
#[must_use]
pub fn names_beside(
    destination: &std::path::Path,
    author_target_identity_digest: &str,
    selected_environment_revision: &str,
    payload: &StagedPayload,
) -> StagingNames {
    let directory = destination.parent().unwrap_or_else(|| std::path::Path::new("."));
    let stem = stem(author_target_identity_digest, selected_environment_revision, payload);
    StagingNames {
        lock: directory.join(format!("{stem}{LOCK_SUFFIX}")),
        sidecar: directory.join(format!("{stem}{SIDECAR_SUFFIX}")),
        staging: directory.join(format!("{stem}{STAGING_SUFFIX}")),
    }
}

/// Why a transfer could not take the lock.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LockRefusal {
    /// Another process holds it.
    #[error("another command is already staging this transfer")]
    Held,
    /// The lock file could not be created or opened.
    #[error("the lock beside this destination could not be taken")]
    Unavailable,
}

/// One held lock, released when it is dropped.
#[derive(Debug)]
pub struct StagingLock {
    /// The file the lock is held on.
    file: std::fs::File,
}

impl StagingLock {
    /// Takes the lock at `path`, or says who has it.
    ///
    /// Exclusive and non-blocking. Waiting would turn a second invocation into
    /// one that eventually starts, which is worse than one that says another is
    /// already running: the caller cannot tell the two apart from outside.
    ///
    /// # Errors
    ///
    /// Returns [`LockRefusal::Held`] or [`LockRefusal::Unavailable`].
    pub fn take(path: &std::path::Path) -> Result<Self, LockRefusal> {
        use fs4::FileExt;
        let file = std::fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(path)
            .map_err(|_| LockRefusal::Unavailable)?;
        match FileExt::try_lock(&file) {
            Ok(()) => Ok(Self { file }),
            Err(_) => Err(LockRefusal::Held),
        }
    }

    /// Returns the file the lock is held on.
    #[must_use]
    pub fn file(&self) -> &std::fs::File {
        &self.file
    }
}

impl Drop for StagingLock {
    fn drop(&mut self) {
        use fs4::FileExt;
        FileExt::unlock(&self.file).ok();
    }
}
