//! Turning one command result into what a protocol client receives.
//!
//! The projection is exact: a client and a command line reading the same result
//! see the same values, because both read the one validated document rather
//! than two renderings of it. Anything else would make the two surfaces
//! disagree about what happened, and whichever one a person checked second
//! would look like the bug.
//!
//! # A failure is content, not a protocol error
//!
//! A malformed request is a protocol error, because the protocol could not read
//! it. A tool that ran and reported a failure is a tool result, because the
//! call worked and the failure is what it found. Collapsing the two would tell
//! a client to retry its transport when the author refused its work.
//!
//! # What is true of the operation decides the error flag
//!
//! An attached call whose operation ended badly is an error to the caller who
//! is waiting for it. An observation that reports the same operation is not:
//! the observation succeeded, and the state it reports is the answer. A client
//! that treated a successful status read as a failure would retry the read.

use serde_json::{Value, json};

use crate::machine_outcome_envelope::MachineOutcomeEnvelope;
use crate::machine_readable_renderer;

/// The tags this protocol never renders as a tool result.
///
/// All four say that a command line stopped: they describe a keystroke at a
/// terminal, and this server has no terminal. A protocol client that wants a
/// call to stop cancels it, which suppresses the answer rather than inventing
/// one that says the call was interrupted.
pub const SUPPRESSED_TAGS: &[&str] = &["local_application_error"];

/// The tags an attached call reports as an error to its caller.
pub const FAILING_TAGS: &[&str] = &["operation_terminal_error"];

/// Why one outcome is not projected.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProjectionRefusal {
    /// The outcome describes a command line stopping, which this is not.
    #[error("{0} describes a command line stopping, which a protocol client never sees")]
    Suppressed(String),
    /// The outcome is larger than one message may carry.
    #[error("this outcome is larger than one message carries: {0}")]
    TooLarge(String),
}

/// What a client receives for one outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolResult {
    /// Whether the caller should read this as their call having failed.
    pub is_error: bool,
    /// The one validated document, as the command line sees it.
    pub structured_content: Value,
    /// The same document as text, byte for byte.
    pub text: String,
    /// Addresses this answer names, offered as links.
    pub resource_links: Vec<String>,
}

/// Returns what a client receives for one envelope.
///
/// The text is the command line's own rendering of the same envelope, and the
/// structured content is that text read back. Producing the two separately
/// would produce two documents that agree today, and a client comparing them
/// tomorrow would find they do not.
///
/// # Errors
///
/// Returns [`ProjectionRefusal::Suppressed`] for an outcome that describes a
/// command line stopping.
pub fn projected(
    envelope: &MachineOutcomeEnvelope,
    attached: bool,
    resource_links: Vec<String>,
) -> Result<ToolResult, ProjectionRefusal> {
    let tag = envelope.tag();
    if SUPPRESSED_TAGS.contains(&tag.as_str()) {
        return Err(ProjectionRefusal::Suppressed(tag));
    }
    let text = machine_readable_renderer::render(envelope)
        .map_err(|refusal| ProjectionRefusal::TooLarge(refusal.to_string()))?;
    let structured_content = serde_json::from_str(&text).unwrap_or(json!({}));
    Ok(ToolResult {
        is_error: attached && FAILING_TAGS.contains(&tag.as_str()),
        structured_content,
        text,
        resource_links,
    })
}

/// Returns whether one observation reports something without failing.
///
/// A status read that finds a failed operation succeeded: it answered the
/// question it was asked. Only the caller waiting on the operation itself is
/// told their call failed.
#[must_use]
pub fn observation_succeeded(result: &ToolResult) -> bool {
    !result.is_error
}
