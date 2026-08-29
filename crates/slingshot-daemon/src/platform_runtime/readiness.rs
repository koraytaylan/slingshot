//! The readiness record of one runtime namespace.
//!
//! Readiness is published atomically by writing a temporary file in the same
//! directory and replacing the record in one operation, so a reader sees either
//! the previous record or the whole new one. The record is a diagnostic: it
//! never confers ownership, and it is removed only by the owner whose nonce it
//! carries, so a departing owner cannot remove a replacement's record.

use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use slingshot_local_protocol::foundation_contract::FoundationContract;

use crate::platform_runtime::failure::PlatformFailure;

/// File-name suffix of the readiness record of one runtime namespace.
pub const READINESS_SUFFIX: &str = ".readiness.json";

/// File-name suffix of the temporary record an atomic publication writes first.
pub const READINESS_TEMPORARY_SUFFIX: &str = ".readiness.pending";

/// What a live daemon publishes about itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadinessRecord {
    /// Process identifier of the daemon, as a diagnostic and never as authority.
    pub process_identifier: u32,
    /// Live readiness nonce, rendered in lowercase hexadecimal.
    pub readiness_nonce: String,
    /// Display form of the endpoint the daemon listens on.
    pub endpoint_display: String,
}

/// Returns the path of the readiness record of one runtime namespace.
#[must_use]
pub fn record_path(runtime_root: &Path, namespace_digest: &str) -> PathBuf {
    runtime_root.join(format!("{namespace_digest}{READINESS_SUFFIX}"))
}

/// Publishes one readiness record atomically.
///
/// # Errors
///
/// Returns [`PlatformFailure::ReadinessRecordTooLarge`] when the rendered
/// record is beyond the contract bound, and [`PlatformFailure::RuntimeState`]
/// when the record cannot be written or replaced.
pub fn publish(
    contract: &FoundationContract,
    runtime_root: &Path,
    namespace_digest: &str,
    record: &ReadinessRecord,
) -> Result<(), PlatformFailure> {
    let rendered = serde_json::to_vec(record).map_err(|failure| PlatformFailure::RuntimeState {
        path: runtime_root.to_path_buf(),
        reason: failure.to_string(),
    })?;
    let limit = contract.namespace.readiness_record_bytes as usize;
    if rendered.len() > limit {
        return Err(PlatformFailure::ReadinessRecordTooLarge { length: rendered.len(), limit });
    }
    let pending = runtime_root.join(format!("{namespace_digest}{READINESS_TEMPORARY_SUFFIX}"));
    let published = record_path(runtime_root, namespace_digest);
    let state = |path: &Path, reason: String| PlatformFailure::RuntimeState {
        path: path.to_path_buf(),
        reason,
    };
    let mut file =
        std::fs::File::create(&pending).map_err(|failure| state(&pending, failure.to_string()))?;
    file.write_all(&rendered).map_err(|failure| state(&pending, failure.to_string()))?;
    file.sync_all().map_err(|failure| state(&pending, failure.to_string()))?;
    drop(file);
    std::fs::rename(&pending, &published).map_err(|failure| state(&published, failure.to_string()))
}

/// Reads the readiness record of one runtime namespace, if one is published.
///
/// # Errors
///
/// Returns [`PlatformFailure::RuntimeState`] when a record exists but cannot be
/// read or is not a readable record.
pub fn read(
    runtime_root: &Path,
    namespace_digest: &str,
) -> Result<Option<ReadinessRecord>, PlatformFailure> {
    let path = record_path(runtime_root, namespace_digest);
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(failure) if failure.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(failure) => {
            return Err(PlatformFailure::RuntimeState { path, reason: failure.to_string() });
        }
    };
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|failure| PlatformFailure::RuntimeState { path, reason: failure.to_string() })
}

/// Removes the readiness record only when it carries the supplied nonce.
///
/// A departing owner therefore cannot remove the record a replacement
/// published, even when the replacement reuses its numeric process identifier.
///
/// # Errors
///
/// Returns [`PlatformFailure::RuntimeState`] when the record exists but cannot
/// be read or removed.
pub fn remove_matching(
    runtime_root: &Path,
    namespace_digest: &str,
    readiness_nonce: &str,
) -> Result<bool, PlatformFailure> {
    let Some(record) = read(runtime_root, namespace_digest)? else {
        return Ok(false);
    };
    if record.readiness_nonce != readiness_nonce {
        return Ok(false);
    }
    let path = record_path(runtime_root, namespace_digest);
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(true),
        Err(failure) if failure.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(failure) => Err(PlatformFailure::RuntimeState { path, reason: failure.to_string() }),
    }
}
