//! Answering about one operation without ever making a client infer.
//!
//! Every variant these queries can return is reached from a real row, and the
//! assertions are about which variant rather than about a message. That is the
//! point: a client matching on a shape cannot mistake work that has not ended
//! for work that failed, and cannot read a proven remote success as an unknown.

use slingshot_daemon::operation_queries::{
    OperationResult, OperationStatus, QueryFailure, result, status,
};
use slingshot_domain::command_fingerprint::{CommandFingerprint, FingerprintInput};
use slingshot_domain::installation::InstallationIdentifier;
use slingshot_domain::operation::{
    OperationExecutionCertainty, OperationFact, OperationLifecycleState, RecoveryCategory,
    RecoveryExecutionEvidence, RecoveryFact, TerminalFailure, TerminalFailureDisposition,
    TerminalFailureKind,
};
use slingshot_storage::database::{OperationDatabase, RequiredSettings};
use slingshot_storage::operation_repository::{
    AdmissionOutcome, AdmissionRequest, OperationRepository, OperationSummary, ResultDisposition,
};

/// Bytes one page occupies, from the runtime contract.
const PAGE_BYTES: u64 = 4096;

/// Pages the database may reach, from the runtime contract.
const DATABASE_PAGES: u64 = 262_144;

/// Milliseconds a busy connection waits, from the runtime contract.
const BUSY_TIMEOUT: u64 = 5000;

/// Two-character pairs in a sixty-four-character hexadecimal value.
const DIGEST_PAIRS: usize = 32;

/// Characters a sixty-four-character hexadecimal value has.
const DIGEST_CHARACTERS: usize = 64;

/// One instant, for a test that does not care which.
const NOW: u64 = 1_700_000_000_000;

/// The environment revision these fixtures are admitted under.
const REVISION: &str = "revision-1";

/// The first partition these fixtures use.
const FIRST_PRINCIPAL: &str = "1d";

/// A second partition, for proving a lookup cannot cross one.
const SECOND_PRINCIPAL: &str = "2d";

/// Returns the settings every connection is held to.
fn settings() -> RequiredSettings {
    RequiredSettings {
        page_bytes: PAGE_BYTES,
        database_pages: DATABASE_PAGES,
        busy_timeout_milliseconds: BUSY_TIMEOUT,
    }
}

/// Returns one repository over a database held in memory.
fn repository() -> OperationRepository {
    OperationRepository::new(OperationDatabase::open_in_memory(settings()).expect("a database"))
}

/// Returns the digest one principal's author target has.
fn partition(principal: &str) -> String {
    principal.repeat(DIGEST_PAIRS)
}

/// Admits one operation and returns the row it made.
fn admitted(repository: &OperationRepository, digest: &str, identifier: &str) -> OperationSummary {
    let canonical = format!("{{\"paths\":[\"/{identifier}\"]}}");
    let asked = AdmissionRequest {
        author_target_identity: format!("opaque-identity-behind-{digest}"),
        author_target_identity_digest: digest.to_owned(),
        caller_identity: Some("caller-1".to_owned()),
        canonical_command: canonical.clone(),
        command_fingerprint: CommandFingerprint::derive(&FingerprintInput {
            author_target_identity_digest: digest.to_owned(),
            canonical_command: canonical,
            command_wire_name: "query_paths".to_owned(),
            command_semantic_contract_version: "1".to_owned(),
            selected_environment_revision: REVISION.to_owned(),
        })
        .expect("a derivable fingerprint"),
        command_wire_name: "query_paths".to_owned(),
        daemon_runtime_contract_digest: "c".repeat(DIGEST_CHARACTERS),
        installation_identifier: InstallationIdentifier::parse(&"a1".repeat(DIGEST_PAIRS))
            .expect("a legal identifier"),
        operation_identifier: identifier.to_owned(),
        selected_environment_revision: REVISION.to_owned(),
        workflow_correlation_identifier: Some("workflow-1".to_owned()),
    };
    let outcome = repository.admit(&asked, NOW).expect("an admission");
    assert!(matches!(outcome, AdmissionOutcome::Admitted(_)), "each fixture admits");
    outcome.summary().clone()
}

/// Walks one operation to `reached`, one legal step at a time.
fn advance(
    repository: &OperationRepository,
    digest: &str,
    identifier: &str,
    from: &OperationSummary,
    reached: OperationLifecycleState,
) -> OperationSummary {
    let path = [
        OperationLifecycleState::Submitting,
        OperationLifecycleState::Accepted,
        OperationLifecycleState::Running,
        OperationLifecycleState::Succeeded,
    ];
    let stop = path.iter().position(|state| *state == reached).expect("a reachable state");
    let mut current = from.clone();
    for state in &path[..=stop] {
        current = repository
            .apply(
                digest,
                identifier,
                current.record.revision,
                &OperationFact::Lifecycle { lifecycle_state: *state },
                NOW,
            )
            .expect("a legal advance");
    }
    current
}

#[test]
fn every_lifecycle_state_reports_exactly_what_the_row_holds() {
    let repository = repository();
    let digest = partition(FIRST_PRINCIPAL);
    let states = [
        OperationLifecycleState::Submitting,
        OperationLifecycleState::Accepted,
        OperationLifecycleState::Running,
    ];
    for (index, reached) in states.iter().enumerate() {
        let identifier = format!("operation-{index}");
        let first = admitted(&repository, &digest, &identifier);
        let advanced = advance(&repository, &digest, &identifier, &first, *reached);

        let held: OperationStatus = status(&repository, &digest, &identifier).expect("a status");
        assert_eq!(held.lifecycle_state, *reached, "the state the row reached");
        assert_eq!(held.revision, advanced.record.revision, "at the revision it is at");
        assert_eq!(held.selected_environment_revision, REVISION, "with its own provenance");
        assert_eq!(held.workflow_correlation_identifier.as_deref(), Some("workflow-1"));

        assert_eq!(
            result(&repository, &digest, &identifier).expect("a result"),
            OperationResult::Pending { lifecycle_state: *reached },
            "and work that has not ended reads as work that has not ended"
        );
    }
}

#[test]
fn a_recovery_reports_its_own_evidence_and_never_reads_as_an_ending() {
    let repository = repository();
    let digest = partition(FIRST_PRINCIPAL);
    let unresolved = RecoveryExecutionEvidence::ExecutionCertainty {
        certainty: OperationExecutionCertainty::SubmissionUnknown,
    };
    let proven = RecoveryExecutionEvidence::AuthoritativeRemoteSuccess;
    let pairs = [
        (RecoveryCategory::AmbiguousSubmission, unresolved),
        (RecoveryCategory::EventReconnection, unresolved),
        (RecoveryCategory::OperationLookup, unresolved),
        (RecoveryCategory::JobSnapshotRecovery, unresolved),
        (RecoveryCategory::ResultAcquisition, proven),
        (RecoveryCategory::ArtifactTransfer, proven),
        (RecoveryCategory::PersistentCapacityUnavailable, proven),
    ];

    for (index, (category, evidence)) in pairs.iter().enumerate() {
        let identifier = format!("recovering-{index}");
        let first = admitted(&repository, &digest, &identifier);
        repository
            .apply(
                &digest,
                &identifier,
                first.record.revision,
                &OperationFact::Recovery {
                    recovery: RecoveryFact {
                        attempt_count: 1,
                        category: *category,
                        detail: "outstanding".to_owned(),
                        evidence: *evidence,
                        manual_resume_eligible: true,
                        retry_delay_milliseconds: 0,
                        retry_observed_at_unix_milliseconds: NOW,
                    },
                },
                NOW,
            )
            .expect("a recovery fact");

        let produced = result(&repository, &digest, &identifier).expect("a result");
        let OperationResult::RecoveryRequired { recovery } = produced else {
            panic!("{category:?} is not an ending: {produced:?}");
        };
        assert_eq!(recovery.category, *category);
        assert_eq!(recovery.evidence, *evidence, "carrying only the evidence its kind admits");
        assert!(
            status(&repository, &digest, &identifier)
                .expect("a status")
                .outstanding_recovery
                .is_some(),
            "and status says it is waiting on something"
        );
    }
}

#[test]
fn every_terminal_pairing_reports_the_one_disposition_its_kind_admits() {
    let repository = repository();
    let digest = partition(FIRST_PRINCIPAL);
    let indeterminate = TerminalFailureDisposition::FailClosedIndeterminate {
        certainty: OperationExecutionCertainty::RemoteOutcomeUnknown,
    };
    let not_executed = TerminalFailureDisposition::AuthoritativeNonExecution {
        certainty: OperationExecutionCertainty::ConfirmedNotExecuted,
    };
    let pairings = [
        (TerminalFailureKind::Rejected, not_executed),
        (TerminalFailureKind::RemoteFailed, TerminalFailureDisposition::AuthoritativeRemoteFailure),
        (
            TerminalFailureKind::ResultUnavailable,
            TerminalFailureDisposition::AuthoritativeRemoteSuccess,
        ),
        (TerminalFailureKind::RecoveryWindowExpired, indeterminate),
        (TerminalFailureKind::RemoteStateLost, indeterminate),
        (TerminalFailureKind::IntegrityFailure, indeterminate),
        (TerminalFailureKind::RetryPolicyExhausted, not_executed),
    ];

    for (index, (kind, disposition)) in pairings.iter().enumerate() {
        let identifier = format!("failing-{index}");
        let first = admitted(&repository, &digest, &identifier);
        repository
            .apply(
                &digest,
                &identifier,
                first.record.revision,
                &OperationFact::Terminal {
                    failure: TerminalFailure {
                        disposition: *disposition,
                        kind: *kind,
                        metadata: None,
                    },
                },
                NOW,
            )
            .expect("a settlement");

        let produced = result(&repository, &digest, &identifier).expect("a result");
        let OperationResult::Failed { failure } = produced else {
            panic!("{kind:?} ended the operation: {produced:?}");
        };
        assert_eq!(failure.kind, *kind);
        assert_eq!(failure.disposition, *disposition, "with no certainty invented for it");
    }
}

#[test]
fn a_success_says_where_its_result_is_and_a_row_that_does_not_is_refused() {
    let repository = repository();
    let digest = partition(FIRST_PRINCIPAL);
    let first = admitted(&repository, &digest, "operation-1");
    let running =
        advance(&repository, &digest, "operation-1", &first, OperationLifecycleState::Succeeded);

    let refused = result(&repository, &digest, "operation-1");
    assert!(
        matches!(refused, Err(QueryFailure::ResultDispositionMissing)),
        "a success whose row does not say where the result went is refused rather than \
         answered with a guess: {refused:?}"
    );

    repository
        .record_result_disposition(
            &digest,
            "operation-1",
            running.record.revision,
            ResultDisposition::Artifact,
        )
        .expect("a disposition");
    assert_eq!(
        result(&repository, &digest, "operation-1").expect("a result"),
        OperationResult::Succeeded { disposition: ResultDisposition::Artifact },
        "and once it does, the answer says which it was"
    );
}

#[test]
fn a_lookup_never_crosses_a_partition_and_absence_is_its_own_answer() {
    let repository = repository();
    let here = partition(FIRST_PRINCIPAL);
    let elsewhere = partition(SECOND_PRINCIPAL);
    admitted(&repository, &here, "operation-1");

    let missing = status(&repository, &elsewhere, "operation-1");
    assert!(
        matches!(missing, Err(QueryFailure::NoSuchOperation { ref identifier })
            if identifier == "operation-1"),
        "the same identifier at another target is another operation, and it is not there"
    );
    let missing = result(&repository, &elsewhere, "operation-1");
    assert!(
        matches!(missing, Err(QueryFailure::NoSuchOperation { .. })),
        "and asking for its result says so rather than reporting pending work"
    );

    admitted(&repository, &elsewhere, "operation-1");
    assert_eq!(
        status(&repository, &elsewhere, "operation-1")
            .expect("a status")
            .author_target_identity_digest,
        elsewhere,
        "once it exists, each answer names the partition it came from"
    );
}

#[test]
fn reopening_the_database_yields_the_same_answers() {
    let directory = tempfile::tempdir().expect("a directory");
    let path = directory.path().join("operations.sqlite3");
    let digest = partition(FIRST_PRINCIPAL);
    let before = {
        let repository = OperationRepository::new(
            OperationDatabase::open(&path, settings()).expect("a database"),
        );
        let first = admitted(&repository, &digest, "operation-1");
        repository
            .apply(
                &digest,
                "operation-1",
                first.record.revision,
                &OperationFact::Terminal {
                    failure: TerminalFailure {
                        disposition: TerminalFailureDisposition::AuthoritativeRemoteSuccess,
                        kind: TerminalFailureKind::ResultUnavailable,
                        metadata: Some("the artifact never arrived".to_owned()),
                    },
                },
                NOW,
            )
            .expect("a settlement");
        (
            status(&repository, &digest, "operation-1").expect("a status"),
            result(&repository, &digest, "operation-1").expect("a result"),
        )
    };

    let repository = OperationRepository::new(
        OperationDatabase::open(&path, settings()).expect("a reopened database"),
    );
    assert_eq!(status(&repository, &digest, "operation-1").expect("a status"), before.0);
    assert_eq!(
        result(&repository, &digest, "operation-1").expect("a result"),
        before.1,
        "a restart changes what these answers say about nothing at all"
    );
}
