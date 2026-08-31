//! The service a person uses to remove work that has ended.
//!
//! Two calls, and the gap between them is the point. Preview says exactly what
//! would go and changes nothing; apply quotes the digest of what was previewed
//! and does exactly that, or refuses. A person therefore acts on something they
//! read, and a target that moved on in between produces a refusal rather than a
//! removal nobody reviewed.
//!
//! Nothing here happens on its own. There is no age policy, no pressure
//! trigger, and no automatic pruning, because the record of what a system did
//! is not something to reclaim space from without being asked.

use slingshot_storage::database::OperationDatabase;
use slingshot_storage::maintenance::{
    self, ApplyOutcome, MaintenanceFailure, TerminalMaintenanceManifest, maximum_removals,
};

/// What a person asked to be previewed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewRequest {
    /// The partition to look at.
    pub author_target_identity_digest: String,
    /// Only operations that settled before this instant are eligible.
    pub before_unix_milliseconds: u64,
    /// At most this many operations, whatever the contract allows above it.
    pub limit: u64,
}

/// What one preview found, and what applying it would digest to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaintenancePreview {
    /// The digest an apply has to quote.
    pub digest: String,
    /// Exactly what would be removed.
    pub manifest: TerminalMaintenanceManifest,
}

/// Returns what one maintenance run would remove.
///
/// The limit is the smaller of what was asked for and what the contract allows,
/// so a request for more than one run may remove is answered with one run's
/// worth rather than refused - a person asking to tidy up everything should get
/// a first instalment, not an error.
///
/// # Errors
///
/// Returns [`MaintenanceFailure`] when the database refuses.
pub fn preview(
    database: &OperationDatabase,
    request: &PreviewRequest,
) -> Result<MaintenancePreview, MaintenanceFailure> {
    let manifest = maintenance::preview(
        database,
        &request.author_target_identity_digest,
        request.before_unix_milliseconds,
        request.limit.min(maximum_removals()),
    )?;
    Ok(MaintenancePreview { digest: manifest.digest(), manifest })
}

/// Applies exactly the manifest a person reviewed.
///
/// # Errors
///
/// Returns [`MaintenanceFailure::ManifestChanged`] when the target moved on
/// since the preview, or a repository failure.
pub fn apply(
    database: &OperationDatabase,
    reviewed: &MaintenancePreview,
    now_unix_milliseconds: u64,
) -> Result<ApplyOutcome, MaintenanceFailure> {
    maintenance::apply(database, &reviewed.manifest, now_unix_milliseconds)
}
