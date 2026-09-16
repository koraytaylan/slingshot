//! Refusal boundaries for activation of a persisted recovery receipt.

use slingshot_domain::operation::{RecoveryCategory, RecoveryResumeReceipt};
use slingshot_storage::agent_job_repository::AgentSubmission;
use slingshot_storage::operation_repository::{OperationRepository, RepositoryFailure};

/// Every supplied receipt field is evidence, not caller-controlled authority.
pub(super) fn assert_refusals_preserve_pause(
    local: &OperationRepository,
    retained: &AgentSubmission,
    receipt: &RecoveryResumeReceipt,
) {
    let identity = &retained.identity;
    let before = local
        .read(&identity.author_target_identity_digest, &identity.operation_identifier)
        .unwrap();
    let mut wrong_operation = receipt.clone();
    wrong_operation.operation_identifier.push_str("-other");
    let mut wrong_environment = receipt.clone();
    wrong_environment.selected_environment_revision.push_str("-other");
    let mut wrong_source = receipt.clone();
    wrong_source.source_fingerprint.push_str("-other");
    let mut wrong_revision = receipt.clone();
    wrong_revision.applied_operation_revision += 1;
    let mut wrong_time = receipt.clone();
    wrong_time.recorded_at_unix_milliseconds -= 1;
    let mut changed_child = retained.clone();
    changed_child.snapshot_watermark = slingshot_domain::remote_job::JobEventSequence::of(u64::MAX);
    let candidates = [
        (
            retained,
            &wrong_operation,
            RecoveryCategory::OperationLookup,
            receipt.recorded_at_unix_milliseconds,
        ),
        (
            retained,
            &wrong_environment,
            RecoveryCategory::OperationLookup,
            receipt.recorded_at_unix_milliseconds,
        ),
        (
            retained,
            &wrong_source,
            RecoveryCategory::OperationLookup,
            receipt.recorded_at_unix_milliseconds,
        ),
        (
            retained,
            &wrong_revision,
            RecoveryCategory::OperationLookup,
            receipt.recorded_at_unix_milliseconds,
        ),
        (
            retained,
            &wrong_time,
            RecoveryCategory::OperationLookup,
            receipt.recorded_at_unix_milliseconds,
        ),
        (
            &changed_child,
            receipt,
            RecoveryCategory::OperationLookup,
            receipt.recorded_at_unix_milliseconds,
        ),
        (
            retained,
            receipt,
            RecoveryCategory::EventReconnection,
            receipt.recorded_at_unix_milliseconds,
        ),
        (
            retained,
            receipt,
            RecoveryCategory::OperationLookup,
            receipt.recorded_at_unix_milliseconds - 1,
        ),
    ];
    for (expected, offered, category, observed_at) in candidates {
        assert!(matches!(
            local.activate_retained_recovery(expected, offered, category, observed_at),
            Err(RepositoryFailure::RemoteObservationMoved)
        ));
        assert_eq!(
            local
                .read(&identity.author_target_identity_digest, &identity.operation_identifier)
                .unwrap(),
            before,
            "a refused activation must preserve the entire paused operation"
        );
        assert_eq!(
            local
                .read_resume_receipt(
                    &identity.author_target_identity_digest,
                    &identity.operation_identifier,
                    &receipt.source_fingerprint,
                )
                .unwrap()
                .as_ref(),
            Some(receipt),
            "a refused activation must preserve the persisted receipt"
        );
    }
}
