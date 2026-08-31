//! Writing an outcome for something that will parse it.
//!
//! Byte-stable is the requirement rather than merely valid. A consumer diffing
//! two runs must see a difference only where one exists, so the field order is
//! fixed, the bytes of an embedded semantic failure are carried through
//! unchanged, and nothing is pretty-printed differently depending on what it
//! contains.
//!
//! # A failure object is embedded, never described
//!
//! When the agent reports a semantic failure, its object goes into the envelope
//! as it arrived. Replacing the category with prose, inferring a missing field,
//! or dropping a budget would leave a consumer unable to branch on the thing it
//! was given the category for.

use crate::machine_outcome_envelope::{
    MAXIMUM_MACHINE_OUTCOME_ENVELOPE_BYTES, MachineOutcomeEnvelope,
};

/// Why an envelope could not be rendered.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RenderRefusal {
    /// The rendered envelope is larger than one may be.
    #[error("a machine outcome holds at most {allowed} bytes, and this holds {actual}")]
    TooLarge {
        /// How large one may be.
        allowed: u64,
        /// How large this is.
        actual: u64,
    },
}

/// Returns the bytes one envelope renders to.
///
/// One line, canonical field order, no trailing space. A consumer reading a
/// stream of these can split on newlines without a parser, which is the
/// difference between a format a shell script can use and one it cannot.
///
/// # Errors
///
/// Returns [`RenderRefusal::TooLarge`] past the bound, which is an invariant
/// violation rather than a value to truncate: a truncated envelope is not a
/// smaller answer, it is an unparseable one.
pub fn render(envelope: &MachineOutcomeEnvelope) -> Result<String, RenderRefusal> {
    let text = serde_json::to_string(envelope).unwrap_or_default();
    let actual = u64::try_from(text.len()).unwrap_or(u64::MAX);
    if actual > MAXIMUM_MACHINE_OUTCOME_ENVELOPE_BYTES {
        return Err(RenderRefusal::TooLarge {
            allowed: MAXIMUM_MACHINE_OUTCOME_ENVELOPE_BYTES,
            actual,
        });
    }
    Ok(text)
}

/// Which stream one rendered outcome is written to.
///
/// Standard output carries answers and standard error carries everything else,
/// so a caller may redirect one without losing the other. An answer on standard
/// error would be invisible to a pipeline, and a diagnostic on standard output
/// would corrupt one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stream {
    /// Answers.
    StandardOutput,
    /// Everything else.
    StandardError,
}

/// Where a machine-readable run writes its one envelope.
///
/// Standard output, whatever the envelope says - including a local error. A run
/// writes exactly one envelope, and sending some of them elsewhere would make a
/// consumer read two streams to find the one answer.
pub const MACHINE_STREAM: Stream = Stream::StandardOutput;
