//! Writing an outcome for a person reading a terminal.
//!
//! Separate from the machine rendering because the two have different
//! obligations. This one may summarize, order things for reading, and leave out
//! what a person already knows; the other may not. Trying to serve both from
//! one renderer means one of them is served badly, and it is always this one,
//! because the other has a schema arguing for it.
//!
//! # An interruption says one of three things
//!
//! What a person can act on after an interrupt depends entirely on how far it
//! got, and the three templates say exactly that: nothing was submitted, the
//! operation exists and is named, or the transfer stopped and can be resumed.
//! A fourth message that hedged between them would leave a person unsure which
//! of the three situations they are in, which is the only thing they need.
//!
//! # Nothing here reaches for a value it was not given
//!
//! A redacted configuration value, a filter that was refused, a package that
//! was not built: the renderer prints the failure it was handed and does not
//! reconstruct what might have been there.

use crate::machine_outcome_envelope::{Interruption, MachineOutcomeEnvelope};
use crate::machine_readable_renderer::Stream;

/// What a person is told when nothing was submitted.
pub const PRE_RECEIPT_TEMPLATE: &str = "interrupted before the daemon answered; quoting {identifier} will say whether \
     anything was admitted";

/// What they are told when the operation exists.
pub const POST_RECEIPT_TEMPLATE: &str =
    "interrupted while watching {identifier}; the operation is running and can be watched again";

/// What they are told when a transfer stopped.
pub const TRANSFER_TEMPLATE: &str = "interrupted while fetching {identifier}; nothing was written where it was going, and \
     running the same command again resumes it";

/// The placeholder each template fills.
const IDENTIFIER_PLACEHOLDER: &str = "{identifier}";

/// One rendered outcome, and where each part of it goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HumanOutput {
    /// What a person reads on standard error.
    pub standard_error: String,
    /// What a pipeline reads on standard output.
    pub standard_output: String,
}

impl HumanOutput {
    /// Returns which stream carries the substance of this outcome.
    #[must_use]
    pub fn substantive_stream(&self) -> Stream {
        if self.standard_output.is_empty() { Stream::StandardError } else { Stream::StandardOutput }
    }
}

/// Returns what a person reads for one envelope.
///
/// An interruption writes nothing to standard output. A pipeline that captured
/// a partial answer would treat it as the answer, and the whole point of an
/// interruption is that there is not one yet.
#[must_use]
pub fn render(envelope: &MachineOutcomeEnvelope) -> HumanOutput {
    match envelope {
        MachineOutcomeEnvelope::LocalApplicationError { interruption } => HumanOutput {
            standard_error: interruption_line(interruption),
            standard_output: String::new(),
        },
        other => HumanOutput { standard_error: String::new(), standard_output: summary(other) },
    }
}

/// Returns the one line an interruption prints.
fn interruption_line(interruption: &Interruption) -> String {
    let (template, identifier) = match interruption {
        Interruption::PreReceipt { retry_identifier } => {
            (PRE_RECEIPT_TEMPLATE, retry_identifier.clone())
        }
        Interruption::PostReceipt { operation_identifier, .. } => {
            (POST_RECEIPT_TEMPLATE, operation_identifier.clone())
        }
        Interruption::ArtifactTransfer { artifact_identifier, .. } => {
            (TRANSFER_TEMPLATE, artifact_identifier.clone())
        }
        Interruption::MaintenanceResultTransfer { maintenance_result_identifier, .. } => {
            (TRANSFER_TEMPLATE, maintenance_result_identifier.clone())
        }
    };
    template.replace(IDENTIFIER_PLACEHOLDER, &identifier)
}

/// Returns the line a person reads for an ordinary outcome.
fn summary(envelope: &MachineOutcomeEnvelope) -> String {
    match envelope {
        MachineOutcomeEnvelope::OperationReceipt { operation_identifier, replayed, revision } => {
            let admitted = if *replayed { "already held" } else { "accepted" };
            format!("{operation_identifier} {admitted} at revision {revision}")
        }
        MachineOutcomeEnvelope::OperationStatus { revision, state } => {
            format!("{state} at revision {revision}")
        }
        MachineOutcomeEnvelope::OperationTerminalError { disposition, kind, .. } => {
            format!("{kind}, {disposition}")
        }
        MachineOutcomeEnvelope::OperationRecoveryRequired { category, evidence, revision } => {
            format!("waiting in {category} at revision {revision}, {evidence}")
        }
        MachineOutcomeEnvelope::DaemonControl { action, state } => format!("{action}: {state}"),
        MachineOutcomeEnvelope::ConfigurationReport { environment, profile, resolved } => {
            let outcome = if *resolved { "resolves" } else { "does not resolve" };
            format!("{profile}/{environment} {outcome}")
        }
        other => other.tag().replace('_', " "),
    }
}
