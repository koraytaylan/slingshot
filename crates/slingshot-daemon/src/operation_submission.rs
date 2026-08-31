//! Turning one execute request into one durable operation, or refusing to.
//!
//! The order matters more than any single check. Identity is verified before
//! the repository is opened, and the executor's availability is settled before
//! a fingerprint is derived, so a request that cannot possibly run never
//! reaches durable state at all. A refusal that had already written a row would
//! leave a client with an operation it could find, wait on, and reasonably ask
//! about, describing work nothing was ever going to do.
//!
//! Admission itself is the repository's: one transaction that writes the row as
//! queued or hands back the one already there. What this module adds is the
//! decision about whether to get that far, and the settlement afterwards.
//!
//! Settlement is where the interesting distinction lives. Work that ended is
//! recorded as ended, with the kind-and-disposition pairing the domain
//! validates. Work that has not ended is recorded as not ended - and the case
//! worth stating on its own is a remote that provably succeeded whose result
//! cannot be stored. That is not a failure and it is not a success: the
//! operation stays nonterminal under `PersistentCapacityUnavailable` carrying
//! `AuthoritativeRemoteSuccess`, publishes no result slot, and keeps its
//! fingerprint, so resuming it retries the local half and never the remote one.

use slingshot_domain::command_fingerprint::{CommandFingerprint, FingerprintInput};
use slingshot_domain::operation::{
    OperationFact, RecoveryCategory, RecoveryExecutionEvidence, RecoveryFact,
};
use slingshot_domain::operation_executor::OperationExecutorOutcome;
use slingshot_storage::operation_repository::{
    AdmissionOutcome, AdmissionRequest, OperationRepository, OperationSummary, RepositoryFailure,
    ResultDisposition,
};

/// What a client asked this daemon to run, and who it thinks it is asking.
///
/// The fingerprint inside the admission is not read from here. This daemon
/// derives it from the canonical command, the target, and the revision it
/// actually serves, because a fingerprint is what decides whether a repeat is
/// the same work - and a client that could supply one could make two different
/// commands look like one repeat, or one repeat look like two commands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecuteRequest {
    /// The admission this request would make.
    pub admission: AdmissionRequest,
    /// The semantic contract version the command is declared under.
    pub command_semantic_contract_version: String,
    /// The runtime contract digest the client expects.
    pub expected_daemon_runtime_contract_digest: String,
}

/// Why an execute request was refused before it reached durable state.
///
/// Every one of these leaves no row. That is the property they share and the
/// reason they are one type: a client that receives any of them knows nothing
/// was created, without having to ask.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SubmissionRefusal {
    /// This build composes no executor, so nothing can run.
    #[error("this daemon composes no operation executor, so it admitted nothing")]
    ExecutionUnavailable,
    /// The client expects a different author target.
    #[error("this daemon serves another author target, and admitted nothing")]
    TargetMismatch,
    /// The client expects a different environment revision.
    #[error("this daemon runs at another environment revision, and admitted nothing")]
    RevisionMismatch,
    /// The client expects a different runtime contract.
    #[error("this daemon runs under another runtime contract, and admitted nothing")]
    ContractMismatch,
}

/// What this daemon is, for a request to be checked against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServedTarget {
    /// The partition it serves.
    pub author_target_identity_digest: String,
    /// The runtime contract it runs under.
    pub daemon_runtime_contract_digest: String,
    /// Whether this build can run anything at all.
    pub execution_available: bool,
    /// The environment revision it started from.
    pub selected_environment_revision: String,
}

impl ServedTarget {
    /// Requires `request` to be one this daemon may admit.
    ///
    /// Availability first. A client asking a build that runs nothing gets the
    /// same answer whatever target it asked about, and finding out costs no
    /// fingerprint and no database access.
    ///
    /// # Errors
    ///
    /// Returns [`SubmissionRefusal`] naming what differs. Nothing is written on
    /// any of these paths.
    pub fn require_admissible(&self, request: &ExecuteRequest) -> Result<(), SubmissionRefusal> {
        if !self.execution_available {
            return Err(SubmissionRefusal::ExecutionUnavailable);
        }
        if request.admission.author_target_identity_digest != self.author_target_identity_digest {
            return Err(SubmissionRefusal::TargetMismatch);
        }
        if request.admission.selected_environment_revision != self.selected_environment_revision {
            return Err(SubmissionRefusal::RevisionMismatch);
        }
        if request.expected_daemon_runtime_contract_digest != self.daemon_runtime_contract_digest {
            return Err(SubmissionRefusal::ContractMismatch);
        }
        Ok(())
    }
}

/// What submitting one request did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubmissionOutcome {
    /// A new operation was admitted.
    Admitted(Box<OperationSummary>),
    /// The same work was already admitted, and this is that row.
    Replayed(Box<OperationSummary>),
    /// The identifier names different work, and nothing changed.
    Conflict(Box<OperationSummary>),
    /// Nothing was admitted, and this says why.
    Refused(SubmissionRefusal),
}

/// Submits one execute request against `repository`.
///
/// Checks identity, then admits. Nothing between those two steps writes
/// anything, so every refusal above is a refusal that created no row.
///
/// # Errors
///
/// Returns [`RepositoryFailure`] when the database refuses. A refusal of the
/// request itself is an outcome rather than an error, because "this daemon
/// serves something else" is a fact the client needs, not a fault.
pub fn submit(
    served: &ServedTarget,
    repository: &OperationRepository,
    request: &ExecuteRequest,
    now_unix_milliseconds: u64,
) -> Result<SubmissionOutcome, RepositoryFailure> {
    if let Err(refusal) = served.require_admissible(request) {
        return Ok(SubmissionOutcome::Refused(refusal));
    }
    let admission = AdmissionRequest {
        command_fingerprint: fingerprint_of(request),
        ..request.admission.clone()
    };
    let admitted = repository.admit(&admission, now_unix_milliseconds)?;
    Ok(match admitted {
        AdmissionOutcome::Admitted(summary) => SubmissionOutcome::Admitted(summary),
        AdmissionOutcome::Replayed(summary) => SubmissionOutcome::Replayed(summary),
        AdmissionOutcome::Conflict(summary) => SubmissionOutcome::Conflict(summary),
    })
}

/// Returns the fingerprint this daemon derives for `request`.
///
/// Derived rather than accepted. Everything it is derived from is a value this
/// daemon has already checked, so two requests fingerprint alike exactly when
/// they are the same command against the same target at the same revision.
#[must_use]
pub fn fingerprint_of(request: &ExecuteRequest) -> CommandFingerprint {
    CommandFingerprint::derive(&FingerprintInput {
        author_target_identity_digest: request.admission.author_target_identity_digest.clone(),
        canonical_command: request.admission.canonical_command.clone(),
        command_wire_name: request.admission.command_wire_name.clone(),
        command_semantic_contract_version: request.command_semantic_contract_version.clone(),
        selected_environment_revision: request.admission.selected_environment_revision.clone(),
    })
    .unwrap_or_else(|failure| {
        panic!("every value a checked request carries is fingerprintable: {failure}")
    })
}

/// Records what one execution produced, as the fact it is.
///
/// Three outcomes reach three different kinds of record, and the middle one is
/// the one worth being careful about. Success settles the operation and says
/// where its result went. A terminal failure settles it with the pairing the
/// domain validates. Recovery-required settles nothing: it records what is
/// outstanding and leaves the operation for the scheduler to come back to.
///
/// # Errors
///
/// Returns [`RepositoryFailure`] when the revision moved or the database
/// refuses.
pub fn settle(
    repository: &OperationRepository,
    summary: &OperationSummary,
    outcome: &OperationExecutorOutcome,
    now_unix_milliseconds: u64,
) -> Result<OperationSummary, RepositoryFailure> {
    let digest = &summary.author_target_identity_digest;
    let identifier = &summary.operation_identifier;
    match outcome {
        OperationExecutorOutcome::Succeeded { artifacts, inline_result } => {
            let disposition = if inline_result.is_some() && artifacts.is_empty() {
                ResultDisposition::Inline
            } else {
                ResultDisposition::Artifact
            };
            let settled = repository.apply(
                digest,
                identifier,
                summary.record.revision,
                &OperationFact::Lifecycle {
                    lifecycle_state:
                        slingshot_domain::operation::OperationLifecycleState::Succeeded,
                },
                now_unix_milliseconds,
            )?;
            repository.record_result_disposition(
                digest,
                identifier,
                settled.record.revision,
                disposition,
            )
        }
        OperationExecutorOutcome::TerminalFailure { failure } => repository.apply(
            digest,
            identifier,
            summary.record.revision,
            &OperationFact::Terminal { failure: failure.clone() },
            now_unix_milliseconds,
        ),
        OperationExecutorOutcome::RecoveryRequired { recovery } => repository.apply(
            digest,
            identifier,
            summary.record.revision,
            &OperationFact::Recovery { recovery: recovery.clone() },
            now_unix_milliseconds,
        ),
    }
}

/// Returns the recovery fact a result that cannot be stored produces.
///
/// Spelled out here because getting it wrong is easy and consequential. The
/// remote succeeded, so the evidence is `AuthoritativeRemoteSuccess` and there
/// is no certainty field to fill in: inventing one would rewrite proven work as
/// work that might not have happened. It is manually resumable because what is
/// missing is capacity, and capacity is something a person can go and free.
#[must_use]
pub fn capacity_unavailable(
    detail: &str,
    attempt_count: u32,
    now_unix_milliseconds: u64,
) -> RecoveryFact {
    RecoveryFact {
        attempt_count,
        category: RecoveryCategory::PersistentCapacityUnavailable,
        detail: detail.to_owned(),
        evidence: RecoveryExecutionEvidence::AuthoritativeRemoteSuccess,
        manual_resume_eligible: true,
        retry_delay_milliseconds: 0,
        retry_observed_at_unix_milliseconds: now_unix_milliseconds,
    }
}
