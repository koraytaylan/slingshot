//! The versioned operation vocabulary, frozen apart from the control surface.
//!
//! Every envelope here carries four facts before it carries a request: the
//! operation-protocol version, the daemon-runtime contract digest, the target
//! the caller believes it is talking to, and the exact environment revision it
//! resolved. A daemon checks all four before it touches its repository, its
//! executor, or the network, because a request that reached the wrong daemon is
//! not a request to answer carefully - it is one to refuse.
//!
//! The target is carried as an opaque digest. A user name, an organization
//! identifier, or a client identifier would be the same fact in a readable
//! form, and these envelopes cross a local socket that anything on the machine
//! can reach.
//!
//! # Two things this vocabulary deliberately keeps apart
//!
//! Bytes travel only in chunk responses. No status, list, wait, result, or
//! maintenance response carries content, so a caller reading a hundred-megabyte
//! package cannot do it by accident inside a status poll.
//!
//! Maintenance results are keyed by target and identifier alone. Their requests
//! carry no operation identifier, no artifact slot, no path, and no offset,
//! because a maintenance result outlives the operation that produced it and a
//! reader that needed the operation could not find it afterwards.
//!
//! # Evidence, and what it is allowed to claim
//!
//! Recovery and terminal failure carry a union rather than an optional field.
//! An unresolved execution says how uncertain it is; a proven remote success
//! says nothing about certainty, because there is nothing left to be uncertain
//! about. Making those two shapes different types is what stops a response from
//! claiming both, or neither.

use serde::{Deserialize, Serialize};

/// How sure the daemon is that a remote command ran.
///
/// Never says a remote command succeeded. That is a different fact, carried by
/// a different shape, and folding the two together is how a retry replays
/// something that already happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationExecutionCertainty {
    /// It provably did not run.
    ConfirmedNotExecuted,
    /// Whether it was submitted is unknown.
    SubmissionUnknown,
    /// It was submitted and its outcome is unknown.
    RemoteOutcomeUnknown,
}

/// What is known about execution when recovery is required.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "evidence", rename_all = "snake_case", deny_unknown_fields)]
pub enum RecoveryExecutionEvidence {
    /// Execution is unproved, and this is how unproved.
    ExecutionCertainty {
        /// How sure the daemon is.
        certainty: OperationExecutionCertainty,
    },
    /// The remote command succeeded and local work remains.
    ///
    /// Carries no certainty field, because there is nothing left to be
    /// uncertain about and a field that could say otherwise would eventually.
    AuthoritativeRemoteSuccess,
}

/// Why an operation ended without succeeding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalFailureKind {
    /// The remote refused it.
    Rejected,
    /// The remote ran it and it failed.
    RemoteFailed,
    /// It succeeded and its result could not be obtained.
    ResultUnavailable,
    /// Recovery ran out of time.
    RecoveryWindowExpired,
    /// The remote no longer knows about it.
    RemoteStateLost,
    /// Something did not verify.
    IntegrityFailure,
    /// Retrying stopped being allowed.
    RetryPolicyExhausted,
}

/// What a terminal failure asserts about effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "disposition", rename_all = "snake_case", deny_unknown_fields)]
pub enum TerminalFailureDisposition {
    /// It provably did not run.
    AuthoritativeNonExecution {
        /// Always [`OperationExecutionCertainty::ConfirmedNotExecuted`].
        certainty: OperationExecutionCertainty,
    },
    /// It ran and failed.
    AuthoritativeRemoteFailure,
    /// It ran and succeeded, and something after that did not.
    AuthoritativeRemoteSuccess,
    /// Nobody can tell, so the daemon fails closed.
    FailClosedIndeterminate {
        /// Which unknown this is.
        certainty: OperationExecutionCertainty,
    },
}

impl TerminalFailureDisposition {
    /// Returns whether the certainty this disposition carries is one it may.
    ///
    /// A non-execution claims exactly one thing and an indeterminate claims one
    /// of two others. The remaining two carry no certainty at all, so there is
    /// no combination left to get wrong.
    #[must_use]
    pub fn is_consistent(self) -> bool {
        match self {
            Self::AuthoritativeNonExecution { certainty } => {
                certainty == OperationExecutionCertainty::ConfirmedNotExecuted
            }
            Self::FailClosedIndeterminate { certainty } => matches!(
                certainty,
                OperationExecutionCertainty::SubmissionUnknown
                    | OperationExecutionCertainty::RemoteOutcomeUnknown
            ),
            Self::AuthoritativeRemoteFailure | Self::AuthoritativeRemoteSuccess => true,
        }
    }
}

/// Reason an operation message could not be built.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MessageFailure {
    /// A digest is not sixty-four lowercase hexadecimal characters.
    #[error("a digest is exactly sixty-four lowercase hexadecimal characters")]
    DigestNotCanonical,
    /// An identifier is empty or over its bound.
    #[error("an identifier is nonempty and inside its named bound")]
    IdentifierOutOfBounds,
    /// A revision that must be present is zero.
    #[error("an expected revision is at least one")]
    RevisionAbsent,
    /// A terminal failure carries a certainty its disposition cannot.
    #[error("a terminal failure carries only the certainty its disposition allows")]
    DispositionInconsistent,
    /// A chunk is larger than the contract allows.
    #[error("a chunk carries at most the contract's decoded byte maximum")]
    ChunkTooLarge,
}

/// Characters a rendered digest occupies.
const DIGEST_CHARACTERS: usize = 64;

/// Returns whether `digest` is sixty-four lowercase hexadecimal characters.
#[must_use]
pub fn digest_is_canonical(digest: &str) -> bool {
    digest.len() == DIGEST_CHARACTERS
        && digest
            .chars()
            .all(|character| character.is_ascii_hexdigit() && !character.is_ascii_uppercase())
}

/// One page of a stream, whichever stream it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChunkBody {
    /// Bytes of this chunk, in the standard padded alphabet.
    pub encoded_bytes: String,
    /// Where these bytes begin in the whole stream.
    pub starting_byte_offset: u64,
}

/// What a caller asks a daemon to do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "request", rename_all = "snake_case", deny_unknown_fields)]
pub enum OperationRequest {
    /// Run one command.
    Execute {
        /// The command, as its typed payload.
        command: serde_json::Value,
        /// Identifier the caller created for this operation.
        operation_identifier: String,
        /// Correlation the caller's workflow supplied, when it has one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        workflow_correlation_identifier: Option<String>,
    },
    /// Report one operation's current state.
    OperationStatus {
        /// Operation to report on.
        operation_identifier: String,
    },
    /// Report a bounded page of operations.
    ListOperations {
        /// Opaque cursor from a previous page, when continuing one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cursor: Option<String>,
        /// Lifecycle states to include.
        #[serde(default)]
        lifecycle_states: Vec<String>,
        /// Operations this page may carry.
        page_size: u32,
    },
    /// Report changes to one operation until it settles.
    Wait {
        /// Operation to watch.
        operation_identifier: String,
    },
    /// Report one settled operation's result.
    Result {
        /// Operation to report on.
        operation_identifier: String,
    },
    /// Resume one operation's recovery.
    ///
    /// Carries the exact revision and category it expects, so a resume issued
    /// against a state the daemon has since left is refused rather than applied
    /// to whatever it found instead.
    ResumeOperationRecovery {
        /// Revision the caller expects the operation to be at.
        expected_operation_revision: u64,
        /// Category the caller expects to be resuming.
        expected_recovery_category: String,
        /// Operation to resume.
        operation_identifier: String,
    },
    /// Read part of one artifact.
    ArtifactRead {
        /// Artifact to read.
        artifact_identifier: String,
        /// Digest the caller expects those bytes to have.
        expected_content_digest: String,
        /// Operation that produced it.
        operation_identifier: String,
        /// Decoded bytes the caller would like back.
        preferred_chunk_bytes: u32,
        /// Where to start.
        starting_byte_offset: u64,
    },
    /// Describe one maintenance result.
    ///
    /// Keyed by target and identifier alone. No operation, slot, path, offset,
    /// or expected digest, because a maintenance result outlives the operation
    /// that produced it.
    MaintenanceResultMetadata {
        /// Target the result belongs to.
        author_target_identity_digest: String,
        /// Result to describe.
        maintenance_result_identifier: String,
    },
    /// Read part of one maintenance result.
    MaintenanceResultRead {
        /// Target the result belongs to.
        author_target_identity_digest: String,
        /// Digest the caller expects the whole document to have.
        expected_content_digest: String,
        /// Result to read.
        maintenance_result_identifier: String,
        /// Decoded bytes the caller would like back.
        preferred_chunk_bytes: u32,
        /// Where to start.
        starting_byte_offset: u64,
    },
    /// Describe what terminal maintenance would do.
    TerminalMaintenancePreview {
        /// Target to preview.
        author_target_identity_digest: String,
        /// Operations the preview may cover.
        maximum_operations: u32,
    },
    /// Apply the maintenance one preview described.
    TerminalMaintenanceApply {
        /// Target to apply to.
        author_target_identity_digest: String,
        /// Digest of the preview being applied.
        reviewed_manifest_digest: String,
    },
}

impl OperationRequest {
    /// Returns the operation this request names, when it names one.
    ///
    /// Maintenance requests name none, and that is the point rather than an
    /// omission.
    #[must_use]
    pub fn operation_identifier(&self) -> Option<&str> {
        match self {
            Self::Execute { operation_identifier, .. }
            | Self::OperationStatus { operation_identifier }
            | Self::Wait { operation_identifier }
            | Self::Result { operation_identifier }
            | Self::ResumeOperationRecovery { operation_identifier, .. }
            | Self::ArtifactRead { operation_identifier, .. } => Some(operation_identifier),
            _ => None,
        }
    }

    /// Returns whether this request reads a maintenance result.
    #[must_use]
    pub fn is_maintenance_result_request(&self) -> bool {
        matches!(self, Self::MaintenanceResultMetadata { .. } | Self::MaintenanceResultRead { .. })
    }
}

/// What one maintenance result is, without any of its bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaintenanceResultDescription {
    /// Revision of the association that holds it.
    pub association_revision: u64,
    /// Target it belongs to.
    pub author_target_identity_digest: String,
    /// Exact length of the document.
    pub byte_length: u64,
    /// Digest of the document.
    pub content_digest: String,
    /// Whether it previews maintenance or receipts it.
    pub kind: String,
    /// Its name.
    pub maintenance_result_identifier: String,
    /// Always `application/json`.
    pub media_type: String,
    /// Whether the current preview or an application receipt retains it.
    pub retention_owner: String,
    /// Digest of the manifest that was reviewed.
    pub reviewed_source_digest: String,
}

/// What a daemon answers with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "response", rename_all = "snake_case", deny_unknown_fields)]
pub enum OperationResponse {
    /// The operation was admitted.
    Accepted {
        /// Operation that was admitted.
        operation_identifier: String,
    },
    /// The operation was already admitted, with the same facts.
    Replayed {
        /// Operation that was already admitted.
        operation_identifier: String,
    },
    /// The daemon has no executor to run it.
    ExecutorUnavailable,
    /// One operation's current state.
    Status {
        /// Lifecycle state it is in.
        lifecycle_state: String,
        /// Operation being reported on.
        operation_identifier: String,
        /// Revision that state was recorded at.
        operation_revision: u64,
    },
    /// A bounded page of operations.
    ListPage {
        /// Cursor for the next page, when there is one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        next_cursor: Option<String>,
        /// Operations on this page.
        operations: Vec<String>,
    },
    /// Something happened to an operation being watched.
    Progress {
        /// Bounded description of what happened.
        detail: String,
        /// Operation it happened to.
        operation_identifier: String,
        /// Revision it happened at.
        operation_revision: u64,
    },
    /// One recovery resume was applied.
    RecoveryResumeApplied {
        /// State the operation is in now.
        current_lifecycle_state: String,
        /// Operation that was resumed.
        operation_identifier: String,
        /// Receipt the resume was recorded as.
        resume_receipt_identifier: String,
    },
    /// One recovery resume had already been applied.
    ///
    /// Carries the truthful current state rather than the state at the time of
    /// the receipt, because a caller replaying a resume wants to know where the
    /// operation is now.
    RecoveryResumeReplayed {
        /// State the operation is in now.
        current_lifecycle_state: String,
        /// Operation that was resumed.
        operation_identifier: String,
        /// Receipt the earlier resume was recorded as.
        resume_receipt_identifier: String,
    },
    /// The operation needs recovery.
    RecoveryRequired {
        /// Bounded category of what went wrong.
        category: String,
        /// What is known about execution.
        evidence: RecoveryExecutionEvidence,
        /// Operation that needs it.
        operation_identifier: String,
    },
    /// The operation ended without succeeding.
    TerminalFailure {
        /// What it asserts about effect.
        disposition: TerminalFailureDisposition,
        /// Why it ended.
        kind: TerminalFailureKind,
        /// Bounded description.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        metadata: Option<String>,
        /// Operation that ended.
        operation_identifier: String,
    },
    /// An artifact transfer is beginning.
    ArtifactStart {
        /// Artifact being transferred.
        artifact_identifier: String,
        /// Exact length of it.
        byte_length: u64,
        /// Digest of it.
        content_digest: String,
        /// What kind of bytes they are.
        media_type: String,
    },
    /// Part of an artifact.
    ArtifactChunk {
        /// The bytes.
        body: ChunkBody,
    },
    /// An artifact transfer is finished.
    ArtifactEnd,
    /// What one maintenance result is.
    MaintenanceResultMetadata {
        /// The description, carrying no bytes and no path.
        description: MaintenanceResultDescription,
    },
    /// A maintenance-result transfer is beginning.
    MaintenanceResultStart {
        /// The same description the metadata response carries.
        description: MaintenanceResultDescription,
    },
    /// Part of a maintenance result.
    MaintenanceResultChunk {
        /// The bytes.
        body: ChunkBody,
    },
    /// A maintenance-result transfer is finished.
    MaintenanceResultEnd,
    /// What terminal maintenance would do.
    MaintenancePreview {
        /// The complete manifest.
        manifest: serde_json::Value,
        /// Digest of it, which an apply must quote back.
        reviewed_manifest_digest: String,
    },
    /// Maintenance was applied.
    MaintenanceApplied {
        /// Receipt it was recorded as.
        application_receipt_identifier: String,
        /// Result the application produced.
        maintenance_result_identifier: String,
    },
    /// Maintenance had already been applied.
    MaintenanceReplayed {
        /// Receipt the earlier application was recorded as.
        application_receipt_identifier: String,
        /// Result that application produced.
        maintenance_result_identifier: String,
    },
    /// The scheduler has no room.
    SchedulerCapacityExhausted {
        /// Bounded guidance.
        guidance: String,
    },
    /// Persistent storage has no room.
    PersistentCapacityExhausted {
        /// Bounded guidance, naming maintenance.
        guidance: String,
        /// Bytes the limit allows.
        limit_bytes: u64,
        /// Bytes in use.
        used_bytes: u64,
    },
    /// Persistent storage is recovering and refusing writes.
    PersistentStorageBackpressure {
        /// Bounded guidance, naming maintenance.
        guidance: String,
        /// Bytes the limit allows.
        limit_bytes: u64,
        /// Bytes in use.
        used_bytes: u64,
    },
    /// An operation with that identifier exists with other facts.
    IdentifierConflict {
        /// Identifier that conflicts.
        operation_identifier: String,
    },
    /// No operation with that identifier exists.
    MissingOperation {
        /// Identifier that is not there.
        operation_identifier: String,
    },
    /// The operation is not in a state that permits this.
    InvalidTransition {
        /// State it is in.
        lifecycle_state: String,
        /// Operation in question.
        operation_identifier: String,
    },
    /// The daemon serves another target.
    TargetMismatch {
        /// Target it serves.
        author_target_identity_digest: String,
    },
    /// The daemon resolved another environment revision.
    RevisionMismatch {
        /// Revision it resolved.
        selected_environment_revision: String,
    },
    /// The daemon was built against another runtime contract.
    RuntimeContractDigestMismatch {
        /// Digest it was built against.
        daemon_runtime_contract_digest: String,
    },
    /// The daemon serves no operation-protocol version the caller speaks.
    IncompatibleOperationProtocol {
        /// Versions it serves.
        supported_operation_protocol_versions: Vec<u32>,
    },
    /// The frame could not be read as a request.
    MalformedFrame {
        /// Bounded description.
        detail: String,
    },
    /// Something inside the daemon failed.
    InternalFailure {
        /// Bounded description, carrying nothing a command supplied.
        detail: String,
    },
}

impl OperationResponse {
    /// Returns whether this response carries content bytes.
    ///
    /// Exactly two do. Everything else describes, and a caller reading a large
    /// artifact does it deliberately rather than by polling a status.
    #[must_use]
    pub fn carries_bytes(&self) -> bool {
        matches!(self, Self::ArtifactChunk { .. } | Self::MaintenanceResultChunk { .. })
    }
}

/// One versioned request, with everything a daemon checks before answering it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationEnvelope {
    /// Target the caller believes it is talking to.
    pub author_target_identity_digest: String,
    /// Runtime contract the caller was built against.
    pub daemon_runtime_contract_digest: String,
    /// Operation-protocol version this envelope speaks.
    pub operation_protocol_version: u32,
    /// The one request it carries.
    pub request: OperationRequest,
    /// Identifier the caller created for this request.
    pub request_identifier: String,
    /// Environment revision the caller resolved.
    pub selected_environment_revision: String,
}

impl OperationEnvelope {
    /// Requires this envelope to be one a daemon could act on.
    ///
    /// Shape only. Whether the digests match *this* daemon is the daemon's
    /// question, and it answers it with its own distinct response.
    ///
    /// # Errors
    ///
    /// Returns [`MessageFailure::DigestNotCanonical`] for a digest that is not
    /// sixty-four lowercase hexadecimal characters,
    /// [`MessageFailure::IdentifierOutOfBounds`] for an empty identifier, and
    /// [`MessageFailure::RevisionAbsent`] for a resume that expects revision
    /// zero.
    pub fn require_well_formed(&self) -> Result<(), MessageFailure> {
        for digest in [
            &self.author_target_identity_digest,
            &self.daemon_runtime_contract_digest,
            &self.selected_environment_revision,
        ] {
            if !digest_is_canonical(digest) {
                return Err(MessageFailure::DigestNotCanonical);
            }
        }
        if self.request_identifier.is_empty() {
            return Err(MessageFailure::IdentifierOutOfBounds);
        }
        if let OperationRequest::ResumeOperationRecovery { expected_operation_revision, .. } =
            &self.request
            && *expected_operation_revision == 0
        {
            return Err(MessageFailure::RevisionAbsent);
        }
        Ok(())
    }
}

/// Writes one value as the payload bytes a frame carries.
///
/// One writer, so two identical typed messages always produce identical bytes -
/// which is what lets a transcript be compared rather than interpreted.
///
/// # Errors
///
/// Returns [`MessageFailure::DigestNotCanonical`] when the value cannot be
/// written, which no valid message provokes.
pub fn write_payload<Value: Serialize>(value: &Value) -> Result<String, MessageFailure> {
    serde_json::to_string(value).map_err(|_| MessageFailure::DigestNotCanonical)
}
