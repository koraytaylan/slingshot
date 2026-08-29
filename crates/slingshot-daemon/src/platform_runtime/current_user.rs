//! The runtime directory one namespace lives in.
//!
//! The directory is reachable only by the current user. On a Unix target the
//! permissions are set as the directory is created, so no other user can see a
//! socket, a lock, or a readiness record even for an instant. On Windows the
//! directory is created inside the current user's own local application data,
//! which the platform already protects with the user's own access-control list.

use std::path::{Path, PathBuf};

use crate::platform_runtime::failure::PlatformFailure;

/// Permission bits of a directory only its owner may read, write, or enter.
#[cfg(unix)]
pub const OWNER_ONLY_DIRECTORY_MODE: u32 = 0o700;

/// Creates the runtime directory of this user, reachable only by this user.
///
/// # Errors
///
/// Returns [`PlatformFailure::RuntimeState`] when the directory cannot be
/// created with the required protection.
#[cfg(unix)]
pub fn create_owner_only_directory(runtime_root: &Path) -> Result<PathBuf, PlatformFailure> {
    use std::os::unix::fs::DirBuilderExt;

    if runtime_root.is_dir() {
        return Ok(runtime_root.to_path_buf());
    }
    if let Some(parent) = runtime_root.parent() {
        std::fs::DirBuilder::new().recursive(true).create(parent).map_err(|failure| {
            PlatformFailure::RuntimeState {
                path: parent.to_path_buf(),
                reason: failure.to_string(),
            }
        })?;
    }
    std::fs::DirBuilder::new().mode(OWNER_ONLY_DIRECTORY_MODE).create(runtime_root).map_err(
        |failure| PlatformFailure::RuntimeState {
            path: runtime_root.to_path_buf(),
            reason: failure.to_string(),
        },
    )?;
    Ok(runtime_root.to_path_buf())
}

/// Creates the runtime directory of this user, reachable only by this user.
///
/// # Errors
///
/// Returns [`PlatformFailure::RuntimeState`] when the directory cannot be
/// created.
#[cfg(windows)]
pub fn create_owner_only_directory(runtime_root: &Path) -> Result<PathBuf, PlatformFailure> {
    std::fs::create_dir_all(runtime_root).map_err(|failure| PlatformFailure::RuntimeState {
        path: runtime_root.to_path_buf(),
        reason: failure.to_string(),
    })?;
    Ok(runtime_root.to_path_buf())
}

/// Reports whether the runtime directory is reachable only by its owner.
///
/// # Errors
///
/// Returns [`PlatformFailure::RuntimeState`] when the directory cannot be
/// inspected.
#[cfg(unix)]
pub fn is_owner_only(runtime_root: &Path) -> Result<bool, PlatformFailure> {
    use std::os::unix::fs::PermissionsExt;

    let metadata =
        std::fs::metadata(runtime_root).map_err(|failure| PlatformFailure::RuntimeState {
            path: runtime_root.to_path_buf(),
            reason: failure.to_string(),
        })?;
    Ok(metadata.permissions().mode() & 0o777 == OWNER_ONLY_DIRECTORY_MODE)
}

/// Reports whether the runtime directory is reachable only by its owner.
///
/// # Errors
///
/// Returns [`PlatformFailure::RuntimeState`] when the directory cannot be
/// inspected.
#[cfg(windows)]
pub fn is_owner_only(runtime_root: &Path) -> Result<bool, PlatformFailure> {
    std::fs::metadata(runtime_root).map(|metadata| metadata.is_dir()).map_err(|failure| {
        PlatformFailure::RuntimeState {
            path: runtime_root.to_path_buf(),
            reason: failure.to_string(),
        }
    })
}
