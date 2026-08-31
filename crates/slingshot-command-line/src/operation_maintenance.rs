//! Listing operations and removing the ones a person reviewed.
//!
//! Nothing here removes anything a caller has not been shown first. A preview
//! produces a digest over exactly what it selected, and an apply quotes that
//! digest and supplies no criteria of its own - so an apply cannot select
//! anything, and a target that moved between the two is a refusal rather than a
//! different removal carried out under an old approval.
//!
//! # A result is referenced when it will not fit
//!
//! A complete manifest is not summarized or truncated to fit a response. When
//! its canonical bytes exceed the shared machine budget, the answer carries the
//! daemon-owned association instead, and the caller fetches it with a second
//! command. Truncating would produce something that looked like a manifest and
//! was not one, which is worse than a reference.
//!
//! # Reading one is metadata first and bytes second
//!
//! The metadata is asked for by target and identifier alone, and every fact it
//! returns is checked - including that its content digest equals the digest the
//! caller supplied. Only then are bytes requested. A read that trusted the
//! caller's digest without the metadata would fetch whatever the identifier now
//! points at; one that trusted the metadata without the caller's digest would
//! fetch whatever the daemon now holds.

use crate::machine_outcome_envelope::MAXIMUM_MACHINE_OUTCOME_ENVELOPE_BYTES;

/// How many operations one page may carry.
pub const MAXIMUM_PAGE_SIZE: u64 = 200;

/// How many operations one maintenance run may select.
pub const MAXIMUM_PREVIEW_LIMIT: u64 = 64;

/// Which partition a maintenance or listing command reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectedPartition {
    /// The one this client currently serves.
    Current {
        /// Its digest.
        author_target_identity_digest: String,
    },
    /// One it served before, which holds only settled work.
    Historical {
        /// Its digest.
        author_target_identity_digest: String,
    },
}

impl SelectedPartition {
    /// Returns the digest either way.
    #[must_use]
    pub fn digest(&self) -> &str {
        match self {
            Self::Current { author_target_identity_digest }
            | Self::Historical { author_target_identity_digest } => author_target_identity_digest,
        }
    }

    /// Returns whether only ended work may be read here.
    #[must_use]
    pub fn holds_only_settled_work(&self) -> bool {
        matches!(self, Self::Historical { .. })
    }
}

/// What one listing asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListingRequest {
    /// What to quote for the page after the first, when there is one.
    pub continuation_token: Option<String>,
    /// How many rows at most.
    pub limit: u64,
    /// Which partition.
    pub partition: SelectedPartition,
}

/// One row of a listing.
///
/// Every row names its partition. A page that omitted it would be ambiguous the
/// moment a caller listed two partitions and kept both answers, which is what a
/// person comparing an old target with a new one does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedOperation {
    /// Which partition it belongs to.
    pub author_target_identity_digest: String,
    /// Which operation.
    pub operation_identifier: String,
    /// Whether it has ended.
    pub settled: bool,
}

/// What one maintenance preview asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewRequest {
    /// Only work settled before this instant is eligible.
    pub before_unix_milliseconds: u64,
    /// How many operations at most.
    pub limit: u64,
    /// Which partition.
    pub partition: SelectedPartition,
}

/// What one maintenance apply asks for.
///
/// A digest and a partition, and nothing that could select anything. An apply
/// that carried criteria could remove something the reviewer never saw, however
/// carefully the criteria matched what they did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyRequest {
    /// Which partition.
    pub partition: SelectedPartition,
    /// The digest the reviewer approved.
    pub reviewed_digest: String,
}

/// Why a listing or maintenance command cannot be made.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MaintenanceRefusal {
    /// A page larger than one may be was asked for.
    #[error("one page carries at most {allowed} rows, and this asked for {actual}")]
    PageTooLarge {
        /// How many one may carry.
        allowed: u64,
        /// How many were asked for.
        actual: u64,
    },
    /// A preview wider than one run may be was asked for.
    #[error("one run selects at most {allowed} operations, and this asked for {actual}")]
    PreviewTooWide {
        /// How many one may select.
        allowed: u64,
        /// How many were asked for.
        actual: u64,
    },
    /// A historical partition was asked for work that has not ended.
    #[error("a historical target holds settled work, and this listing asked for more")]
    HistoryNotSettled,
    /// An apply quoted a digest this target does not have.
    #[error("this target's rows do not digest to what the apply quoted")]
    DigestUnknown,
    /// The caller's digest is not the one the metadata reports.
    #[error("the digest this caller expects is not the digest this result has")]
    DigestMismatched,
    /// The association named by the metadata is no longer readable.
    #[error("that maintenance result has been superseded or retired")]
    NoLongerReadable,
}

/// Requires one listing to be one a page may answer.
///
/// # Errors
///
/// Returns [`MaintenanceRefusal::PageTooLarge`] or
/// [`MaintenanceRefusal::HistoryNotSettled`].
pub fn require_listable(
    request: &ListingRequest,
    includes_unsettled: bool,
) -> Result<(), MaintenanceRefusal> {
    if request.limit > MAXIMUM_PAGE_SIZE {
        return Err(MaintenanceRefusal::PageTooLarge {
            actual: request.limit,
            allowed: MAXIMUM_PAGE_SIZE,
        });
    }
    if request.partition.holds_only_settled_work() && includes_unsettled {
        return Err(MaintenanceRefusal::HistoryNotSettled);
    }
    Ok(())
}

/// Requires one preview to be one a single run may select.
///
/// # Errors
///
/// Returns [`MaintenanceRefusal::PreviewTooWide`].
pub fn require_previewable(request: &PreviewRequest) -> Result<(), MaintenanceRefusal> {
    if request.limit > MAXIMUM_PREVIEW_LIMIT {
        return Err(MaintenanceRefusal::PreviewTooWide {
            actual: request.limit,
            allowed: MAXIMUM_PREVIEW_LIMIT,
        });
    }
    Ok(())
}

/// Requires one apply to quote a digest this target's rows produce now.
///
/// # Errors
///
/// Returns [`MaintenanceRefusal::DigestUnknown`] when the target has moved on,
/// which is a refusal rather than a different removal carried out under an old
/// approval.
pub fn require_appliable(
    request: &ApplyRequest,
    current_digest: &str,
) -> Result<(), MaintenanceRefusal> {
    if request.reviewed_digest != current_digest {
        return Err(MaintenanceRefusal::DigestUnknown);
    }
    Ok(())
}

/// How one maintenance answer travels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    /// Small enough to travel in the answer itself.
    Inline,
    /// Referenced, and fetched with a second command.
    Referenced,
}

/// Returns how a manifest of `canonical_bytes` travels.
///
/// Referenced rather than truncated past the budget. Truncating would produce
/// something that looked like a manifest and was not one, which is worse for a
/// reviewer than being told where to get the whole thing.
#[must_use]
pub fn delivery_of(canonical_bytes: u64) -> Delivery {
    if canonical_bytes <= MAXIMUM_MACHINE_OUTCOME_ENVELOPE_BYTES {
        Delivery::Inline
    } else {
        Delivery::Referenced
    }
}

/// What the daemon says about one maintenance result before its bytes are read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultMetadata {
    /// Which partition it belongs to.
    pub author_target_identity_digest: String,
    /// How many bytes it holds.
    pub byte_length: u64,
    /// What it digests to.
    pub content_digest: String,
    /// Which result.
    pub maintenance_result_identifier: String,
    /// Whether it is still readable.
    pub readable: bool,
}

/// Requires one result to be the one the caller asked for, before bytes move.
///
/// Metadata first, and the caller's digest checked against it. A read that
/// trusted the caller's digest alone would fetch whatever the identifier now
/// points at; one that trusted the metadata alone would fetch whatever the
/// daemon now holds. Requiring both means a result that changed underneath is a
/// refusal rather than a surprise on disk.
///
/// # Errors
///
/// Returns [`MaintenanceRefusal::NoLongerReadable`] or
/// [`MaintenanceRefusal::DigestMismatched`].
pub fn require_readable(
    metadata: &ResultMetadata,
    partition: &SelectedPartition,
    maintenance_result_identifier: &str,
    expected_digest: &str,
) -> Result<(), MaintenanceRefusal> {
    if !metadata.readable
        || metadata.author_target_identity_digest != partition.digest()
        || metadata.maintenance_result_identifier != maintenance_result_identifier
    {
        return Err(MaintenanceRefusal::NoLongerReadable);
    }
    if metadata.content_digest != expected_digest {
        return Err(MaintenanceRefusal::DigestMismatched);
    }
    Ok(())
}
