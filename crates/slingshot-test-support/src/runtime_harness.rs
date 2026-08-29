//! Injected temporary runtime roots and real monotonic waiting.
//!
//! A test never touches the runtime or configuration directories of the user
//! running it. It asks for a temporary root instead, works inside it, and gets
//! it removed when the handle drops. Waiting is polled against the monotonic
//! clock with an explicit deadline, so a test never sleeps for a fixed span and
//! never asserts a lower bound on how long something took.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Interval between two polls while waiting for a real condition.
pub const POLL_INTERVAL: Duration = Duration::from_millis(5);

/// One temporary runtime root, removed when its handle drops.
///
/// The directory name is short on purpose. An endpoint address is bounded by
/// the operating system, and a namespace digest takes most of that bound, so a
/// long root would make a real endpoint unnameable.
#[derive(Debug)]
pub struct TemporaryRuntimeRoot {
    path: PathBuf,
}

impl TemporaryRuntimeRoot {
    /// Creates one temporary runtime root under the system temporary directory.
    ///
    /// `label` must be short; it only distinguishes roots inside one process.
    ///
    /// # Errors
    ///
    /// Returns the operating-system failure that prevented the directory from
    /// being created.
    pub fn create(label: &str) -> std::io::Result<Self> {
        let path = std::env::temp_dir().join(format!("sls{}{label}", std::process::id()));
        std::fs::remove_dir_all(&path).ok();
        std::fs::create_dir_all(&path)?;
        Ok(Self { path })
    }

    /// Returns the path of this temporary runtime root.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Removes this temporary runtime root now.
    ///
    /// # Errors
    ///
    /// Returns the operating-system failure that prevented the removal.
    pub fn remove(&self) -> std::io::Result<()> {
        match std::fs::remove_dir_all(&self.path) {
            Err(failure) if failure.kind() == std::io::ErrorKind::NotFound => Ok(()),
            other => other,
        }
    }
}

impl Drop for TemporaryRuntimeRoot {
    fn drop(&mut self) {
        self.remove().ok();
    }
}

/// Waits until a condition holds, or reports that the deadline elapsed.
///
/// The wait is polled against the monotonic clock. The caller supplies the
/// deadline, so a harness never invents a duration of its own and never sleeps
/// for a fixed span in place of a condition.
pub fn wait_until(deadline: Duration, mut condition: impl FnMut() -> bool) -> bool {
    let started = Instant::now();
    loop {
        if condition() {
            return true;
        }
        if started.elapsed() >= deadline {
            return false;
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}
