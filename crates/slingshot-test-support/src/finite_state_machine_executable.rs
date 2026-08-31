//! Path-only finite-state-machine executable.
//!
//! A value that names an externally built executable by path alone, so a test
//! carries no build logic and no provenance claim. Whoever produced the
//! executable proves that separately; this only says that something at this
//! path exists, is a regular file, and can be run by this process.
//!
//! The three checks are made when the value is created rather than when it is
//! used, because a path that names nothing is a mistake in the scenario and
//! discovering it at the point of use produces a failure that describes the
//! wrong thing.

use std::path::{Path, PathBuf};

/// Why one path does not name a runnable executable.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ExecutableRefusal {
    /// The path is not absolute, so what it names depends on where it is read.
    #[error("{0} is not an absolute path")]
    NotAbsolute(PathBuf),
    /// Nothing at that path is a regular file.
    #[error("{0} is not a regular file")]
    NotARegularFile(PathBuf),
    /// This process cannot run it.
    #[error("{0} is not executable by this process")]
    NotExecutable(PathBuf),
}

/// One externally built executable, named by path alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FiniteStateMachineExecutable {
    /// Where it is.
    path: PathBuf,
}

impl FiniteStateMachineExecutable {
    /// Returns the executable one path names.
    ///
    /// # Errors
    ///
    /// Returns [`ExecutableRefusal`] naming the first check the path fails.
    pub fn at(path: impl Into<PathBuf>) -> Result<Self, ExecutableRefusal> {
        let path = path.into();
        if !path.is_absolute() {
            return Err(ExecutableRefusal::NotAbsolute(path));
        }
        if !path.is_file() {
            return Err(ExecutableRefusal::NotARegularFile(path));
        }
        if !is_executable(&path) {
            return Err(ExecutableRefusal::NotExecutable(path));
        }
        Ok(Self { path })
    }

    /// Returns where it is.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Reports whether this process can run what one path names.
#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    const ANY_EXECUTE_BIT: u32 = 0o111;
    std::fs::metadata(path)
        .map(|held| held.permissions().mode() & ANY_EXECUTE_BIT != 0)
        .unwrap_or(false)
}

/// Reports whether this process can run what one path names.
#[cfg(windows)]
fn is_executable(path: &Path) -> bool {
    path.extension().is_some_and(|held| held.eq_ignore_ascii_case("exe"))
}
