//! Running one tool call as one durable operation.
//!
//! A tool call reaches the same daemon, the same registry command, and the same
//! operation identity a command line would reach, so the two surfaces cannot
//! disagree about what a request did. Everything a command line checks before
//! it sends is checked here in the same order, because a check that happens on
//! one surface and not the other is a hole shaped exactly like that surface.
//!
//! # An invented key is invented once
//!
//! A command that repeats harmlessly may omit its operation key, and this
//! server invents one. It invents it once: the identifier is kept with the
//! request and reused for every reconnect and every retry, because a second
//! identifier would make the retry a second operation and the caller would have
//! started work twice by asking once. It is never derived from the protocol
//! request identifier, which belongs to the transport and can be reused by a
//! client the moment its answer arrives.
//!
//! # A resume names the revision and the category it expects
//!
//! Releasing a recovery is the one control that changes an operation's course,
//! so it says what it believes about the operation first. If the operation has
//! moved on, the belief is stale and the resume schedules nothing - which is
//! what makes an exactly-committed resume safe to send twice.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::model_context_protocol::schema_projection::{
    OPERATION_KEY_MEMBER, ProjectionRefusal, require_acceptable,
};
use crate::model_context_protocol::tool_catalog::{
    KeyPresence, Provenance, ToolDescriptor, derive,
};

/// Where one operation key came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeySource {
    /// The caller supplied it, and it is preserved exactly.
    Supplied(String),
    /// This server invented it once, and reuses it.
    GeneratedOnce(String),
    /// The tool starts no work, so it has none.
    Absent,
}

impl KeySource {
    /// Returns the identifier this source names, when it names one.
    #[must_use]
    pub fn identifier(&self) -> Option<&str> {
        match self {
            Self::Supplied(held) | Self::GeneratedOnce(held) => Some(held),
            Self::Absent => None,
        }
    }
}

/// Why one tool call is not run.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ExecutionRefusal {
    /// The build's provenance does not agree with itself.
    #[error("this build's provenance does not agree: {0}")]
    ProvenanceDrifted(String),
    /// This server offers no such tool.
    #[error("this server offers no tool called {0}")]
    ToolUnknown(String),
    /// The arguments were refused by one of the three checks.
    #[error(transparent)]
    ArgumentsRefused(#[from] ProjectionRefusal),
    /// A resume was asked for without what it has to believe.
    #[error("a resume names the revision and the recovery category it expects")]
    ResumeIncomplete,
}

/// What each active tool request is holding.
///
/// Keyed by the protocol request identifier, because that is what a reconnect
/// arrives quoting. What is held is the operation identity, which is what makes
/// the reconnect the same operation rather than a second one.
#[derive(Debug, Default)]
pub struct ExecutionState {
    /// One entry per active request that invented a key.
    invented: BTreeMap<String, String>,
}

impl ExecutionState {
    /// Returns a state holding nothing.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns where this call's operation key comes from.
    ///
    /// The generator is called at most once per request, and only for a command
    /// that may omit its key. A later call under the same request identifier -
    /// a reconnect, a retry - finds the identifier already held and returns it
    /// unchanged.
    pub fn operation_key(
        &mut self,
        request_identifier: &str,
        tool: &ToolDescriptor,
        supplied: Option<&str>,
        generate: impl FnOnce() -> String,
    ) -> KeySource {
        if let Some(held) = supplied {
            return KeySource::Supplied(held.to_owned());
        }
        if tool.operation_key == KeyPresence::Absent {
            return KeySource::Absent;
        }
        let held =
            self.invented.entry(request_identifier.to_owned()).or_insert_with(generate).clone();
        KeySource::GeneratedOnce(held)
    }

    /// Releases what one finished or detached request was holding.
    pub fn release(&mut self, request_identifier: &str) -> bool {
        self.invented.remove(request_identifier).is_some()
    }

    /// Returns how many requests are holding an invented identifier.
    #[must_use]
    pub fn holding(&self) -> usize {
        self.invented.len()
    }
}

/// Requires one tool call to be one this server runs, and returns its arguments.
///
/// Provenance first, then the tool, then the three argument checks in their own
/// order. Nothing reaches a daemon until all of them pass, because a request
/// that fails any of them is a request the daemon would have to refuse anyway -
/// and it would refuse it after having been told about it.
///
/// # Errors
///
/// Returns [`ExecutionRefusal`] naming the first thing that stops the call.
pub fn require_runnable(
    named: &str,
    raw_arguments: &[u8],
    provenance: &Provenance,
) -> Result<(ToolDescriptor, Value), ExecutionRefusal> {
    let offered = derive(provenance)
        .map_err(|refusal| ExecutionRefusal::ProvenanceDrifted(refusal.to_string()))?;
    let tool = offered
        .into_iter()
        .find(|held| held.name == named)
        .ok_or_else(|| ExecutionRefusal::ToolUnknown(named.to_owned()))?;
    let arguments = require_acceptable(&tool, raw_arguments)?;
    Ok((tool, arguments))
}

/// Returns the operation key one accepted argument document supplies.
#[must_use]
pub fn supplied_key(arguments: &Value) -> Option<&str> {
    arguments.get(OPERATION_KEY_MEMBER).and_then(Value::as_str)
}

/// What this server believes about an operation when it asks to resume it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumeBelief {
    /// Which recovery category the caller believes it waits in.
    pub expected_recovery_category: String,
    /// Which revision the caller believes it stands at.
    pub expected_operation_revision: u64,
    /// Which operation.
    pub operation_identifier: String,
}

/// What one operation actually is when a resume arrives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumeReality {
    /// Which recovery category it waits in, when it waits in one.
    pub recovery_category: Option<String>,
    /// Which revision it stands at.
    pub revision: u64,
    /// Whether a receipt for this release already exists.
    pub receipted: bool,
}

/// What a resume does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResumeOutcome {
    /// It released the recovery, and this is the first time.
    Applied,
    /// It released nothing, because the release already happened.
    Replayed,
    /// It released nothing, and this says why.
    Refused {
        /// What was wrong with the belief.
        detail: String,
    },
}

/// Returns what one resume does against what the operation actually is.
///
/// A stale belief schedules nothing. That is the property that makes a resume
/// safe to send twice: the second one finds the operation already moved and
/// refuses, or finds the receipt and replays, and neither runs anything again.
#[must_use]
pub fn resumed(belief: &ResumeBelief, reality: &ResumeReality) -> ResumeOutcome {
    if reality.receipted {
        return ResumeOutcome::Replayed;
    }
    let Some(category) = reality.recovery_category.as_deref() else {
        return ResumeOutcome::Refused { detail: "it waits in no recovery".to_owned() };
    };
    if category != belief.expected_recovery_category {
        return ResumeOutcome::Refused {
            detail: format!("it waits in {category}, and the resume expects another"),
        };
    }
    if reality.revision != belief.expected_operation_revision {
        return ResumeOutcome::Refused {
            detail: format!(
                "it stands at revision {}, and the resume expects another",
                reality.revision
            ),
        };
    }
    ResumeOutcome::Applied
}

/// Requires one resume to say what it believes before it is sent.
///
/// # Errors
///
/// Returns [`ExecutionRefusal::ResumeIncomplete`] when either belief is absent.
pub fn require_complete_belief(arguments: &Value) -> Result<ResumeBelief, ExecutionRefusal> {
    let identifier = arguments
        .get("operation_identifier")
        .and_then(Value::as_str)
        .ok_or(ExecutionRefusal::ResumeIncomplete)?;
    let category = arguments
        .get("expected_recovery_category")
        .and_then(Value::as_str)
        .ok_or(ExecutionRefusal::ResumeIncomplete)?;
    let revision = arguments
        .get("expected_operation_revision")
        .and_then(Value::as_u64)
        .ok_or(ExecutionRefusal::ResumeIncomplete)?;
    Ok(ResumeBelief {
        expected_recovery_category: category.to_owned(),
        expected_operation_revision: revision,
        operation_identifier: identifier.to_owned(),
    })
}
