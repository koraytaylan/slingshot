//! Failures of the platform runtime.
//!
//! One error type covers every platform operation of a runtime namespace, so a
//! caller handles the same shape whether it is naming an endpoint, taking a
//! lock, publishing readiness, or preparing the runtime directory.

#[cfg(not(any(unix, windows)))]
compile_error!(
    "Slingshot runs only on the target rows in support/platforms.toml: \
     x86_64-unknown-linux-gnu, aarch64-apple-darwin, and x86_64-pc-windows-msvc"
);

use std::path::PathBuf;

/// Reason a platform runtime operation could not be completed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PlatformFailure {
    /// The endpoint name is beyond the bound this platform declares.
    #[error("the endpoint name is {length} long, beyond the limit of {limit}")]
    EndpointNameTooLong {
        /// Length the name reached.
        length: usize,
        /// Largest length the contract allows.
        limit: usize,
    },
    /// The rendered readiness record is beyond the contract bound.
    #[error("the readiness record is {length} bytes, beyond the limit of {limit}")]
    ReadinessRecordTooLarge {
        /// Length the record reached.
        length: usize,
        /// Largest record the contract allows.
        limit: usize,
    },
    /// A lock file could not be opened.
    #[error("the lock at {path} is unavailable: {reason}")]
    LockUnavailable {
        /// Path of the lock file.
        path: PathBuf,
        /// Operating-system reason the lock file could not be opened.
        reason: String,
    },
    /// A runtime state file or directory could not be read or written.
    #[error("the runtime state at {path} could not be used: {reason}")]
    RuntimeState {
        /// Path of the runtime state.
        path: PathBuf,
        /// Operating-system reason the state could not be used.
        reason: String,
    },
}
