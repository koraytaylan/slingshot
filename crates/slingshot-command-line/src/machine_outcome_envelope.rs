//! The envelope every machine-readable answer is written in.
//!
//! One outer shape, whatever command produced it. A shape that varied by
//! command would make every consumer learn every command before it could read
//! an error, and the consumers that mattered would learn only the commands they
//! happened to try first.
//!
//! # The tags are closed and each answer selects exactly one
//!
//! A consumer that met a tag it did not know would have to guess whether the
//! operation succeeded, and both guesses are wrong in the case that matters. So
//! the union is closed, every command leaf maps to one of these, and nothing
//! adds a tag without adding it here.
//!
//! # A local problem may never claim a remote fact
//!
//! The interruption variants are structurally unable to carry terminal evidence
//! or a semantic failure. A command that was interrupted before its receipt
//! arrived knows nothing about the operation, and a shape that let it say
//! otherwise would let a local signal report a remote outcome.

use serde::{Deserialize, Serialize};

/// How large one rendered envelope may be.
///
/// Strictly below the pinned four-kilobyte canonical acknowledgement cap the
/// workflow integration is held to, so an envelope that fits here fits there.
/// It makes no claim that a whole maintenance manifest fits: that is what the
/// access branches exist for.
pub const MAXIMUM_MACHINE_OUTCOME_ENVELOPE_BYTES: u64 = 4000;

/// How long one inline dynamic name may be.
pub const MAXIMUM_INLINE_NAME_BYTES: u64 = 256;

/// The scheme every access reference is written under.
pub const ACCESS_SCHEME: &str = "slingshot";

/// Characters an access reference segment keeps as itself.
const UNRESERVED: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";

/// Where one artifact of one operation can be fetched from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactAccess {
    /// Which artifact.
    pub artifact_identifier: String,
    /// Which partition it belongs to.
    pub author_target_identity_digest: String,
    /// How many bytes it holds.
    pub byte_length: u64,
    /// What it digests to.
    pub content_digest: String,
    /// What it is.
    pub media_type: String,
    /// Which operation produced it.
    pub operation_identifier: String,
    /// Where to ask for it.
    pub uri: String,
}

/// Where one maintenance result can be fetched from.
///
/// No operation and no slot. A maintenance result is an association of a
/// target, and naming an operation would invent one the daemon never made.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaintenanceResultAccess {
    /// Which revision of the association this is.
    pub association_revision: u64,
    /// Which partition it belongs to.
    pub author_target_identity_digest: String,
    /// How many bytes it holds.
    pub byte_length: u64,
    /// What it digests to.
    pub content_digest: String,
    /// What kind of result it is.
    pub kind: String,
    /// Which result.
    pub maintenance_result_identifier: String,
    /// What it is.
    pub media_type: String,
    /// What the reviewer approved.
    pub reviewed_source_digest: String,
    /// Where to ask for it.
    pub uri: String,
}

/// Which phase an interruption arrived in.
///
/// The distinction decides what may honestly be said. Before a receipt nothing
/// is known about any operation; after one the operation exists and is named;
/// during a transfer the operation and artifact are known and the local path is
/// not reported, because where a caller was writing is their business.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "phase", rename_all = "snake_case", deny_unknown_fields)]
pub enum Interruption {
    /// Before the daemon answered, so no operation is claimed.
    PreReceipt {
        /// What to quote to find out what happened.
        retry_identifier: String,
    },
    /// After it answered, so the operation exists and is named.
    PostReceipt {
        /// Which operation.
        operation_identifier: String,
        /// Which revision it was admitted at.
        revision: u64,
    },
    /// While an artifact was being fetched.
    ArtifactTransfer {
        /// Which artifact.
        artifact_identifier: String,
        /// Which operation produced it.
        operation_identifier: String,
    },
    /// While a maintenance result was being fetched.
    MaintenanceResultTransfer {
        /// Which partition it belongs to.
        author_target_identity_digest: String,
        /// Which result.
        maintenance_result_identifier: String,
    },
}

/// One answer, in the one shape every consumer parses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case", deny_unknown_fields)]
pub enum MachineOutcomeEnvelope {
    /// The daemon admitted an operation and this names it.
    OperationReceipt {
        /// Which operation.
        operation_identifier: String,
        /// Whether it was new work.
        replayed: bool,
        /// The revision it stands at.
        revision: u64,
    },
    /// Where an operation has got to.
    OperationStatus {
        /// What state it is in.
        state: String,
        /// The revision that state was read at.
        revision: u64,
    },
    /// It ended, and this is what it produced.
    OperationResult {
        /// The canonical result, exactly as it was validated.
        result: serde_json::Value,
    },
    /// It ended without succeeding.
    OperationTerminalError {
        /// What the daemon says about whether it ran.
        disposition: String,
        /// The semantic failure, exactly as the agent reported it.
        failure: serde_json::Value,
        /// Which closed kind of ending this is.
        kind: String,
    },
    /// It is waiting for somebody.
    OperationRecoveryRequired {
        /// Which recovery category it waits in.
        category: String,
        /// What is known about whether it ran.
        evidence: String,
        /// The revision it stands at.
        revision: u64,
    },
    /// A resume was applied or replayed.
    OperationResumeReceipt {
        /// Which category it released.
        category: String,
        /// Whether it had been applied before.
        replayed: bool,
    },
    /// One page of operations.
    OperationListPage {
        /// The operations on it.
        operations: Vec<String>,
        /// What to quote for the next page, when there is one.
        continuation_token: Option<String>,
    },
    /// Where one artifact of one command's result can be fetched.
    CommandArtifactAccess {
        /// Every access entry the result names.
        artifacts: Vec<ArtifactAccess>,
        /// Every other member of the result, exactly as validated.
        result: serde_json::Value,
    },
    /// Where an over-inline structured result can be fetched.
    StructuredResultArtifactAccess {
        /// The one entry the daemon created.
        artifact: ArtifactAccess,
    },
    /// Where one maintenance result can be fetched.
    MaintenanceResultAccess {
        /// The association.
        access: MaintenanceResultAccess,
    },
    /// What a maintenance run would remove.
    MaintenancePreview {
        /// The digest an apply quotes.
        reviewed_digest: String,
        /// How many operations it would release.
        released_operation_rows: u64,
    },
    /// What the selected configuration resolves to.
    ConfigurationReport {
        /// Which environment.
        environment: String,
        /// Which profile.
        profile: String,
        /// Whether it resolves at all.
        resolved: bool,
    },
    /// What a daemon control command did.
    DaemonControl {
        /// Which action.
        action: String,
        /// What it found or did.
        state: String,
    },
    /// Something local went wrong, and it claims nothing remote.
    LocalApplicationError {
        /// Which phase it happened in.
        interruption: Interruption,
    },
}

impl MachineOutcomeEnvelope {
    /// Every tag a consumer may meet, in the order this file declares them.
    pub const EVERY_TAG: &'static [&'static str] = &[
        "operation_receipt",
        "operation_status",
        "operation_result",
        "operation_terminal_error",
        "operation_recovery_required",
        "operation_resume_receipt",
        "operation_list_page",
        "command_artifact_access",
        "structured_result_artifact_access",
        "maintenance_result_access",
        "maintenance_preview",
        "configuration_report",
        "daemon_control",
        "local_application_error",
    ];

    /// Returns the tag this envelope selects.
    ///
    /// Read from the serialized form rather than restated in a match, so what a
    /// consumer parses and what this reports cannot drift: a variant renamed on
    /// one side and not the other would otherwise be a silent disagreement.
    ///
    /// # Panics
    ///
    /// Panics when the envelope does not serialize to an object carrying its
    /// tag, which no value of this closed enum can do.
    #[must_use]
    pub fn tag(&self) -> String {
        serde_json::to_value(self)
            .ok()
            .and_then(|value| value["outcome"].as_str().map(str::to_owned))
            .expect("a tagged enum serializes to an object carrying its tag")
    }

    /// Returns whether this envelope asserts something about remote execution.
    ///
    /// Only the two that are entitled to. A local error cannot reach this, by
    /// construction rather than by discipline: it has no field to put one in.
    #[must_use]
    pub fn claims_remote_authority(&self) -> bool {
        matches!(self, Self::OperationTerminalError { .. } | Self::OperationResult { .. })
    }
}

/// Returns `segment` with every reserved character percent-encoded, once.
#[must_use]
pub fn encoded_segment(segment: &str) -> String {
    let mut encoded = String::new();
    for octet in segment.bytes() {
        if UNRESERVED.as_bytes().contains(&octet) {
            encoded.push(char::from(octet));
        } else {
            encoded.push_str(&format!("%{octet:02X}"));
        }
    }
    encoded
}

/// Returns where one artifact of one operation is asked for.
#[must_use]
pub fn artifact_uri(
    profile: &str,
    environment: &str,
    author_target_identity_digest: &str,
    operation_identifier: &str,
    artifact_identifier: &str,
) -> String {
    format!(
        "{ACCESS_SCHEME}://profiles/{}/environments/{}/targets/{}/operations/{}/artifacts/{}",
        encoded_segment(profile),
        encoded_segment(environment),
        encoded_segment(author_target_identity_digest),
        encoded_segment(operation_identifier),
        encoded_segment(artifact_identifier)
    )
}

/// Returns where one maintenance result is asked for.
///
/// No operation segment. A maintenance result belongs to a target rather than
/// to any operation, and a reference that named one would be a reference to
/// something the daemon never created.
#[must_use]
pub fn maintenance_result_uri(
    profile: &str,
    environment: &str,
    author_target_identity_digest: &str,
    maintenance_result_identifier: &str,
) -> String {
    format!(
        "{ACCESS_SCHEME}://profiles/{}/environments/{}/targets/{}/maintenance/results/{}",
        encoded_segment(profile),
        encoded_segment(environment),
        encoded_segment(author_target_identity_digest),
        encoded_segment(maintenance_result_identifier)
    )
}
