//! Turning one outcome into the acknowledgement a workflow journals.
//!
//! The executor stores what it is told and advances on it, so an
//! acknowledgement is not a report - it is the durable record a later decision
//! is made from. Two things follow. The structured value is the exact envelope
//! rather than a summary of it, and whether the call succeeded is decided by
//! what the operation did rather than by how the answer is spelled.
//!
//! # A digest appears only when the bytes do not
//!
//! An answer inside the cap travels whole. One past it travels as a nested
//! prefix and its digest, because the executor's record has a size and an
//! answer that would not fit has to be identifiable rather than truncated. A
//! digest beside a whole answer would be a second way to say the same thing,
//! and the two would eventually disagree.
//!
//! # Authority comes from the disposition, never from the spelling
//!
//! Whether a compensating effect may even be considered depends on the daemon
//! having said the work provably ran and failed. A failure category, an error
//! flag, or a message that reads like a failure establishes nothing: those are
//! descriptions, and compensation acts on the world.

use serde_json::{Value, json};

/// The largest structured acknowledgement the executor accepts, in bytes.
pub const ACKNOWLEDGEMENT_CAP_BYTES: usize = 4_096;

/// How many bytes of an over-cap answer travel as a prefix.
pub const NESTED_PREFIX_BYTES: usize = 1_024;

/// The member an over-cap answer's digest is carried in.
pub const DIGEST_MEMBER: &str = "structured_sha256";

/// The member an over-cap answer's prefix is carried in.
pub const PREFIX_MEMBER: &str = "structured_prefix";

/// The slot a deterministic operation artifact occupies, and nothing else does.
pub const OPERATION_ARTIFACT_SLOT: &str = "structured_result";

/// What the daemon says an ending means for the work itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Authority {
    /// It provably did not run.
    AuthoritativeNonExecution,
    /// It ran and failed.
    AuthoritativeRemoteFailure,
    /// It ran and succeeded, and something after that did not.
    AuthoritativeRemoteSuccess,
    /// Nobody can tell.
    FailClosedIndeterminate,
}

impl Authority {
    /// Returns whether this evidence lets a compensating effect be considered.
    ///
    /// One of the four. Compensation undoes something, so it is considered only
    /// where something provably happened and provably failed. Non-execution has
    /// nothing to undo; an indeterminate outcome may have nothing to undo; and a
    /// remote success that failed afterwards is a different problem with a
    /// different answer.
    #[must_use]
    pub fn permits_compensation_review(self) -> bool {
        matches!(self, Self::AuthoritativeRemoteFailure)
    }

    /// Returns the exact spelling the daemon uses for this disposition.
    #[must_use]
    pub fn as_text(self) -> &'static str {
        match self {
            Self::AuthoritativeNonExecution => "AuthoritativeNonExecution",
            Self::AuthoritativeRemoteFailure => "AuthoritativeRemoteFailure",
            Self::AuthoritativeRemoteSuccess => "AuthoritativeRemoteSuccess",
            Self::FailClosedIndeterminate => "FailClosedIndeterminate",
        }
    }

    /// Returns the disposition one exact spelling names.
    #[must_use]
    pub fn named(text: &str) -> Option<Self> {
        [
            Self::AuthoritativeNonExecution,
            Self::AuthoritativeRemoteFailure,
            Self::AuthoritativeRemoteSuccess,
            Self::FailClosedIndeterminate,
        ]
        .into_iter()
        .find(|held| held.as_text() == text)
    }
}

/// What the workflow journals for one call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Acknowledgement {
    /// Which event the machine advances on.
    pub event: String,
    /// Whether the call itself succeeded.
    pub ok: bool,
    /// What is journalled, exactly.
    pub result: Value,
}

impl Acknowledgement {
    /// Returns the structured value this acknowledgement journalled.
    #[must_use]
    pub fn structured(&self) -> &Value {
        &self.result["structured"]
    }

    /// Reports whether the answer travelled whole.
    #[must_use]
    pub fn carried_whole(&self) -> bool {
        self.result["structured"][DIGEST_MEMBER].is_null()
    }
}

/// Returns what a workflow journals for one envelope.
///
/// The envelope is journalled exactly when it fits. When it does not, a nested
/// prefix and the digest of the whole thing are journalled instead, so a later
/// reader can say which answer this was without the executor having had to hold
/// it.
#[must_use]
pub fn acknowledged(envelope: &Value, ok: bool, event: &str) -> Acknowledgement {
    let rendered = serde_json::to_string(envelope).unwrap_or_default();
    let structured = if rendered.len() <= ACKNOWLEDGEMENT_CAP_BYTES {
        envelope.clone()
    } else {
        json!({
            PREFIX_MEMBER: rendered.chars().take(NESTED_PREFIX_BYTES).collect::<String>(),
            DIGEST_MEMBER: digest_of(rendered.as_bytes()),
        })
    };
    Acknowledgement { event: event.to_owned(), ok, result: json!({ "structured": structured }) }
}

/// Returns the lowercase hexadecimal digest of some bytes.
fn digest_of(bytes: &[u8]) -> String {
    use sha2::Digest;
    let mut digest = sha2::Sha256::new();
    digest.update(bytes);
    digest.finalize().iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Which externalized branch one answer took.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Externalization {
    /// A command's own result, in the deterministic operation artifact slot.
    OperationArtifact,
    /// A maintenance result, addressed by target and identifier.
    MaintenanceResult,
    /// Neither: the answer travelled inside the envelope.
    None,
}

/// Returns which externalized branch one envelope took.
///
/// The two branches are disjoint by construction. A maintenance result routed
/// through the operation artifact slot would need an operation identity that
/// does not exist, and the slot is reserved for the one thing that does have
/// one.
#[must_use]
pub fn externalization_of(envelope: &Value) -> Externalization {
    match envelope["outcome"].as_str() {
        Some("structured_result_artifact_access") => Externalization::OperationArtifact,
        Some("maintenance_result_access") => Externalization::MaintenanceResult,
        _ => Externalization::None,
    }
}
