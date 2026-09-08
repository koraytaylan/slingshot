//! Injected temporary runtime roots and real monotonic waiting.
//!
//! A test never touches the runtime or configuration directories of the user
//! running it. It asks for a temporary root instead, works inside it, and gets
//! it removed when the handle drops. Waiting is polled against the monotonic
//! clock with an explicit deadline, so a test never sleeps for a fixed span and
//! never asserts a lower bound on how long something took.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

#[cfg(unix)]
use slingshot_local_protocol::foundation_contract::FoundationContract;

/// Interval between two polls while waiting for a real condition.
pub const POLL_INTERVAL: Duration = Duration::from_millis(5);

/// Prefix used for one short-lived runtime-root directory.
const RUNTIME_ROOT_PREFIX: &str = "sls";

/// Process-local counter that keeps parallel test roots distinct.
static NEXT_RUNTIME_ROOT_IDENTIFIER: AtomicUsize = AtomicUsize::new(0);

/// Counter-width headroom reserved when checking the endpoint path length.
#[cfg(unix)]
const RUNTIME_ROOT_COUNTER_HEADROOM: &str = "9999";

/// Hexadecimal characters in a namespace digest.
#[cfg(unix)]
const NAMESPACE_DIGEST_HEX_CHARACTERS: usize = 64;

/// Suffix appended to a Unix-domain socket endpoint.
#[cfg(unix)]
const UNIX_SOCKET_SUFFIX: &str = ".socket";

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
        let path = runtime_root_path(label);
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

/// Returns a short, process-scoped runtime-root path suitable for endpoint names.
#[must_use]
pub fn runtime_root_path(label: &str) -> PathBuf {
    let identifier = NEXT_RUNTIME_ROOT_IDENTIFIER.fetch_add(1, Ordering::Relaxed);
    temporary_parent().join(format!(
        "{RUNTIME_ROOT_PREFIX}{}{}{label}",
        std::process::id(),
        identifier
    ))
}

/// Chooses a temporary parent that leaves room for a complete Unix endpoint.
#[cfg(unix)]
fn temporary_parent() -> PathBuf {
    let system_parent = std::env::temp_dir();
    if parent_can_hold_endpoint(&system_parent) {
        return system_parent;
    }
    let short_parent = PathBuf::from("/tmp");
    if parent_can_hold_endpoint(&short_parent) {
        return short_parent;
    }
    system_parent
}

/// Chooses the system temporary directory on platforms with named pipes.
#[cfg(not(unix))]
fn temporary_parent() -> PathBuf {
    std::env::temp_dir()
}

/// Reports whether one runtime root leaves room for the namespace endpoint.
#[cfg(unix)]
fn parent_can_hold_endpoint(parent: &Path) -> bool {
    let root_name =
        format!("{RUNTIME_ROOT_PREFIX}{}{}x", std::process::id(), RUNTIME_ROOT_COUNTER_HEADROOM);
    let root = parent.join(root_name);
    let endpoint_bytes =
        root.as_os_str().len() + 1 + NAMESPACE_DIGEST_HEX_CHARACTERS + UNIX_SOCKET_SUFFIX.len();
    let limit = FoundationContract::embedded().namespace.unix_socket_address_bytes as usize;
    endpoint_bytes <= limit
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
