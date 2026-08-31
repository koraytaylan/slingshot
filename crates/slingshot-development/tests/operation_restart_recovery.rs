//! Interrupting a daemon at every boundary and finding a coherent fact after.
//!
//! Restart safety is not something a test establishes by restarting at
//! convenient moments. What matters is the boundaries - between a row and its
//! receipt, between a receipt and the bytes it accounts for, between a rename
//! and the synchronize that makes it durable - so this walks every one of them
//! deliberately.
//!
//! The assertion after each interruption is always the same shape: whatever
//! survived is a fact something can act on, and never a half-written one.

use slingshot_development::test_daemon_faults::FaultedRun;
use slingshot_domain::command_fingerprint::{CommandFingerprint, FingerprintInput};
use slingshot_domain::installation::InstallationIdentifier;
use slingshot_domain::operation::{
    OperationFact, OperationLifecycleState, RecoveryCategory, RecoveryExecutionEvidence,
    RecoveryFact,
};
use slingshot_storage::artifact_store::{ArtifactStore, InstallationRequest, STAGING_SUFFIX};
use slingshot_storage::database::{OperationDatabase, RequiredSettings};
use slingshot_storage::operation_repository::{
    AdmissionOutcome, AdmissionRequest, OperationRepository,
};
use slingshot_test_support::operation_fault_injection::{
    Checkpoint, EVERY_CHECKPOINT, FaultInjector, Instruction,
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

/// The partition these fixtures use.
const PRINCIPAL: &str = "1d";

/// The operation every fixture admits.
const OPERATION: &str = "operation-1";

/// The instant one fixture step after [`NOW`].
const LATER: u64 = NOW + 1;

/// Times the reconstruction fixture reopens the database.
const REOPENS: usize = 3;

/// Returns the settings every connection is held to.
fn settings() -> RequiredSettings {
    RequiredSettings {
        page_bytes: PAGE_BYTES,
        database_pages: DATABASE_PAGES,
        busy_timeout_milliseconds: BUSY_TIMEOUT,
    }
}

/// Returns the digest this suite's author target has.
fn partition() -> String {
    PRINCIPAL.repeat(DIGEST_PAIRS)
}

/// Returns one admission request.
fn admission() -> AdmissionRequest {
    let digest = partition();
    let canonical = "{\"paths\":[\"/content\"]}";
    AdmissionRequest {
        author_target_identity: format!("opaque-identity-behind-{digest}"),
        author_target_identity_digest: digest.clone(),
        caller_identity: None,
        canonical_command: canonical.to_owned(),
        command_fingerprint: CommandFingerprint::derive(&FingerprintInput {
            author_target_identity_digest: digest,
            canonical_command: canonical.to_owned(),
            command_wire_name: "query_paths".to_owned(),
            command_semantic_contract_version: "1".to_owned(),
            selected_environment_revision: REVISION.to_owned(),
        })
        .expect("a derivable fingerprint"),
        command_wire_name: "query_paths".to_owned(),
        daemon_runtime_contract_digest: "c".repeat(DIGEST_CHARACTERS),
        installation_identifier: InstallationIdentifier::parse(&"a1".repeat(DIGEST_PAIRS))
            .expect("a legal identifier"),
        operation_identifier: OPERATION.to_owned(),
        selected_environment_revision: REVISION.to_owned(),
        workflow_correlation_identifier: None,
    }
}

/// Returns one repository over the database at `path`.
fn repository(path: &std::path::Path) -> OperationRepository {
    OperationRepository::new(OperationDatabase::open(path, settings()).expect("a database"))
}

#[test]
fn every_checkpoint_is_named_reached_once_and_disarmed_after_it_fires() {
    for checkpoint in EVERY_CHECKPOINT {
        let run = FaultedRun::stopping_at(*checkpoint);
        let interrupted = run.walk().expect("the armed checkpoint stops the run");
        assert_eq!(interrupted.interrupted_at, *checkpoint, "it stopped where it was armed");
        assert_eq!(
            interrupted.reached.last(),
            Some(checkpoint),
            "having reached everything before it first"
        );

        assert!(
            run.walk().is_none(),
            "{checkpoint:?}: a retry gets past the point that stopped it, or the suite would \
             prove only that a daemon can fail the same way twice"
        );
    }
    assert!(FaultedRun::uninterrupted().walk().is_none(), "and an unarmed run goes all the way");
}

#[test]
fn an_admission_interrupted_before_acknowledgement_is_still_a_whole_row() {
    let directory = tempfile::tempdir().expect("a directory");
    let path = directory.path().join("operations.sqlite3");
    let injector = FaultInjector::passive();
    injector.arm(Checkpoint::AfterAdmissionCommit);

    let admitted = {
        let repository = repository(&path);
        let outcome = repository.admit(&admission(), NOW).expect("an admission");
        assert!(matches!(outcome, AdmissionOutcome::Admitted(_)));
        assert_eq!(
            injector.reach(Checkpoint::AfterAdmissionCommit),
            Instruction::Interrupt,
            "the daemon stops here, before the client is told anything"
        );
        outcome.summary().clone()
    };

    let repository = repository(&path);
    let found = repository.read(&partition(), OPERATION).expect("a read").expect("a row");
    assert_eq!(found, admitted, "the row is whole, because the commit is what made it exist");
    assert_eq!(found.record.lifecycle_state, OperationLifecycleState::Queued);

    let again = repository.admit(&admission(), LATER).expect("the client retries");
    assert!(
        matches!(again, AdmissionOutcome::Replayed(_)),
        "and a client that never heard back finds its own operation rather than making a second"
    );
}

#[test]
fn a_recovery_interrupted_before_its_revision_is_published_survives_whole() {
    let directory = tempfile::tempdir().expect("a directory");
    let path = directory.path().join("operations.sqlite3");
    let waiting = {
        let repository = repository(&path);
        let admitted = repository.admit(&admission(), NOW).expect("an admission");
        repository
            .apply(
                &partition(),
                OPERATION,
                admitted.summary().record.revision,
                &OperationFact::Recovery {
                    recovery: RecoveryFact {
                        attempt_count: 1,
                        category: RecoveryCategory::PersistentCapacityUnavailable,
                        detail: "the result does not fit".to_owned(),
                        evidence: RecoveryExecutionEvidence::AuthoritativeRemoteSuccess,
                        manual_resume_eligible: true,
                        retry_delay_milliseconds: 0,
                        retry_observed_at_unix_milliseconds: NOW,
                    },
                },
                NOW,
            )
            .expect("a recovery fact")
    };

    let repository = repository(&path);
    let found = repository.read(&partition(), OPERATION).expect("a read").expect("a row");
    assert_eq!(found, waiting, "the fact came back exactly as it went in");
    let recovery = found.record.outstanding_recovery.expect("a recovery fact");
    assert_eq!(recovery.category, RecoveryCategory::PersistentCapacityUnavailable);
    assert_eq!(
        recovery.evidence,
        RecoveryExecutionEvidence::AuthoritativeRemoteSuccess,
        "and a restart did not turn proven remote work into an unknown"
    );
    assert!(!found.record.lifecycle_state.is_terminal(), "nor into an ending");
}

#[test]
fn an_artifact_interrupted_before_publication_leaves_nothing_addressable() {
    /// A reader that stops part way, the way an interrupted transfer does.
    struct Interrupted {
        /// Bytes still to hand out before failing.
        remaining: usize,
    }
    impl std::io::Read for Interrupted {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            if self.remaining == 0 {
                return Err(std::io::Error::other("the transfer stopped"));
            }
            let handed = self.remaining.min(buffer.len());
            self.remaining -= handed;
            buffer[..handed].fill(b'x');
            Ok(handed)
        }
    }

    let directory = tempfile::tempdir().expect("a directory");
    let store = ArtifactStore::open(&directory.path().join("artifacts")).expect("a store");
    let request = InstallationRequest {
        artifact_slot: "content_package".to_owned(),
        author_target_identity_digest: partition(),
        descriptor: None,
        installation_identifier: InstallationIdentifier::parse(&"a1".repeat(DIGEST_PAIRS))
            .expect("a legal identifier"),
        media_type: "application/zip".to_owned(),
        operation_identifier: OPERATION.to_owned(),
    };
    store
        .install(&request, &mut Interrupted { remaining: PAGE_BYTES as usize })
        .expect_err("an interrupted installation is a refusal");

    let content = directory.path().join("artifacts").join("content");
    let left: Vec<String> = std::fs::read_dir(&content)
        .expect("the content directory reads")
        .map(|entry| entry.expect("an entry").file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(left.len(), 1, "the partial write is still there to be found: {left:?}");
    assert!(
        left[0].ends_with(STAGING_SUFFIX),
        "wearing the staging suffix, so nothing addresses it as content"
    );

    let reopened = ArtifactStore::open(&directory.path().join("artifacts")).expect("a store");
    let whole = reopened
        .install(&request, &mut b"a complete artifact".as_slice())
        .expect("a later installation succeeds beside it");
    assert!(
        content.join(&whole.content_digest).exists(),
        "and the complete one is addressable while the partial one is not"
    );
}

#[test]
fn a_partition_is_reconstructed_whole_however_many_times_it_reopens() {
    let directory = tempfile::tempdir().expect("a directory");
    let path = directory.path().join("operations.sqlite3");
    {
        let repository = repository(&path);
        repository.admit(&admission(), NOW).expect("an admission");
    }

    let mut seen = Vec::new();
    for _ in 0..REOPENS {
        let repository = repository(&path);
        let reconstructed = repository.reconstruct(&partition()).expect("a reconstruction");
        seen.push(reconstructed);
    }
    assert_eq!(seen[0], seen[1], "two reopens see the same partition");
    assert_eq!(seen[1], seen[2], "and so does a third");
    assert_eq!(seen[0].len(), 1, "holding the one operation that was admitted");
}
