//! Durable facts about work somebody else is running.
//!
//! Two things are proved. The first is that everything a resubmission would
//! have to derive is written down and comes back byte for byte after a reopen:
//! the contracts, the arguments, the revision, the generation, the digest. A
//! restart that could not re-derive those would either refuse to resume
//! anything or resume under a name it had guessed at.
//!
//! The second is that idempotency lives in the statements rather than in
//! anything a caller remembers. A cursor advances only to a later position, a
//! fold applies only to the sequence it expected, an ending lands only on a row
//! that has not ended, and a physical job records the same name twice without
//! complaint. So every replay here is exercised twice and asked to change
//! nothing the second time.
//!
//! Throughout, the subscription ledger is kept honest about the case that makes
//! it awkward: a stream carries events about work this daemon does not hold,
//! and the position must move anyway.

use slingshot_domain::persistent_capacity::PersistentCapacityPolicy;
use slingshot_domain::remote_job::{AgentJobState, JobEventSequence, RemoteJobObservation};
use slingshot_storage::agent_job_repository::{
    AgentCapacityBounds, AgentJobRepository, AgentRepositoryFailure, AgentSubmission,
    BYTES_PER_EVENT, PHYSICAL_JOBS_PER_SUBMISSION, SubmissionContracts, SubmissionIdentity,
    SubmissionOutcome,
};
use slingshot_storage::agent_subscription_ledger::{
    AgentSubscriptionLedger, EventFact, LedgerOutcome,
};
use slingshot_storage::database::{OperationDatabase, RequiredSettings};
use slingshot_storage::maintenance;

/// Where the vectors this suite is driven from live.
const FIXTURES: &str = "tests/fixtures/agent-job-storage";

/// Bytes one page occupies, from the runtime contract.
const PAGE_BYTES: u64 = 4096;

/// Pages the database may reach, from the runtime contract.
const DATABASE_PAGES: u64 = 262_144;

/// Milliseconds a busy connection waits, from the runtime contract.
const BUSY_TIMEOUT: u64 = 5000;

/// The partition every fact here belongs to.
const TARGET: &str = "target-identity-digest-one";

/// Another partition, to prove nothing reaches across.
const ANOTHER_TARGET: &str = "target-identity-digest-two";

/// The subscription carrying these events.
const SUBSCRIPTION: &str = "daemon-subscription-one";

/// The generation these facts belong to.
const GENERATION: u64 = 7;

/// A later generation, after the agent's store was rebuilt.
const LATER_GENERATION: u64 = 8;

/// One instant, for the facts that need one.
const NOW: u64 = 1_700_000_000_000;

/// How long the agent promises to keep one submission's results.
const RETENTION: u64 = 120_000;

/// A sequence a fold advances to.
const SECOND_SEQUENCE: u64 = 2;

/// A later sequence, for a watermark.
const FIFTH_SEQUENCE: u64 = 5;

/// How far along a job that has reported once says it is.
const SOME_PROGRESS: u64 = 40;

/// How many times one disagreement is reported.
const REPEATED_REPORTS: u64 = 3;

/// How many positions a compaction fixture records before compacting.
const RECORDED_POSITIONS: u64 = 4;

/// How many positions a compaction leaves behind.
const RETAINED_POSITIONS: u64 = 2;

/// Returns the settings every database here is opened under.
fn settings() -> RequiredSettings {
    RequiredSettings {
        page_bytes: PAGE_BYTES,
        database_pages: DATABASE_PAGES,
        busy_timeout_milliseconds: BUSY_TIMEOUT,
    }
}

/// Returns one migrated database in memory.
fn migrated() -> OperationDatabase {
    OperationDatabase::open_in_memory(settings()).expect("a migrated database")
}

/// Returns a repository over a fresh in-memory database.
fn repository() -> AgentJobRepository {
    AgentJobRepository::new(migrated())
}

/// Returns a ledger over a fresh in-memory database with one subscription open.
fn ledger() -> AgentSubscriptionLedger {
    let ledger = AgentSubscriptionLedger::new(migrated());
    ledger.open_subscription(TARGET, SUBSCRIPTION, GENERATION, NOW).expect("one subscription");
    ledger
}

/// Returns every vector one fixture holds.
fn vectors(name: &str) -> Vec<serde_json::Value> {
    let path = format!("{FIXTURES}/{name}");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("{path} is readable"));
    text.lines().map(|line| serde_json::from_str(line).expect("each line is one vector")).collect()
}

/// Returns the identity one submission has in `target`.
fn identity_in(target: &str, named: &str) -> SubmissionIdentity {
    SubmissionIdentity {
        agent_event_store_generation: GENERATION,
        agent_operation_identifier: format!("agent-operation-{named}"),
        author_target_identity_digest: target.to_owned(),
        daemon_subscription_identifier: SUBSCRIPTION.to_owned(),
        operation_identifier: format!("local-operation-{named}"),
        selected_environment_revision: "environment-revision-one".to_owned(),
    }
}

/// Returns the contracts one submission was made under.
fn contracts(named: &str) -> SubmissionContracts {
    SubmissionContracts {
        argument_schema_digest: "argument-schema-digest".to_owned(),
        author_agent_transport_contract_digest: "transport-contract-digest".to_owned(),
        command_canonical_json_contract_digest: "canonical-contract-digest".to_owned(),
        command_contract_limits_digest: "limits-digest".to_owned(),
        command_semantic_contract_version: "1".to_owned(),
        command_wire_name: "query_paths".to_owned(),
        result_schema_digest: "result-schema-digest".to_owned(),
        submitted_command_digest: format!("submitted-digest-{named}"),
    }
}

/// Returns one submission against `target`.
fn submission_in(target: &str, named: &str) -> AgentSubmission {
    AgentSubmission {
        canonical_submission: format!("{{\"path\":\"/content/{named}\"}}"),
        contracts: contracts(named),
        identity: identity_in(target, named),
        observation: RemoteJobObservation::accepted(),
        recorded_at_unix_milliseconds: NOW,
        remaining_retention_milliseconds: RETENTION,
        request_start_unix_milliseconds: NOW,
        snapshot_watermark: JobEventSequence::of(0),
        terminal_disposition: None,
    }
}

/// Returns one submission against the partition everything else uses.
fn submission(named: &str) -> AgentSubmission {
    submission_in(TARGET, named)
}

#[test]
fn retained_submission_debug_does_not_disclose_the_stored_request() {
    let repository = repository();
    let mut expected = submission("private-operation-sentinel");
    expected.canonical_submission = r#"{"password":"private-command-sentinel"}"#.to_owned();
    repository.submit(&expected).unwrap();
    let retained =
        repository.read(TARGET, &expected.identity.agent_operation_identifier).unwrap().unwrap();
    assert_eq!(format!("{retained:?}"), "AgentSubmission([redacted])");
    assert_eq!(format!("{retained:#?}"), "AgentSubmission([redacted])");
    assert_eq!(retained, expected);
    assert_eq!(retained.clone().canonical_submission, expected.canonical_submission);
}

#[test]
fn active_snapshot_is_atomic_and_a_stale_read_cannot_advance_it() {
    let repository = repository();
    let expected = submission("snapshot");
    repository.submit(&expected).unwrap();
    let jobs = vec!["job-a".to_owned()];
    let observation = running(2, 1, 10);
    repository
        .reconcile_active_snapshot(&expected, &jobs, observation, RETENTION, NOW + 1)
        .unwrap();
    let identifier = &expected.identity.agent_operation_identifier;
    let retained = repository.read(TARGET, identifier).unwrap().unwrap();
    assert_eq!(retained.observation, observation);
    assert_eq!(retained.snapshot_watermark, JobEventSequence::of(2));
    assert_eq!(repository.physical_jobs(TARGET, identifier).unwrap(), jobs);
    assert!(
        repository
            .reconcile_active_snapshot(
                &expected,
                &["job-b".to_owned()],
                running(3, 2, 20),
                RETENTION,
                NOW + 2
            )
            .is_err()
    );
    assert_eq!(repository.read(TARGET, identifier).unwrap().unwrap(), retained);
    assert_eq!(repository.physical_jobs(TARGET, identifier).unwrap(), jobs);
    let mut terminal = running(3, 2, 20);
    terminal.state = AgentJobState::Succeeded;
    assert!(
        repository
            .reconcile_active_snapshot(&retained, &jobs, terminal, RETENTION, NOW + 2)
            .is_err()
    );
    assert!(
        repository
            .reconcile_active_snapshot(&retained, &jobs, running(2, 1, 11), RETENTION, NOW + 2)
            .is_err()
    );
    assert_eq!(repository.read(TARGET, identifier).unwrap().unwrap(), retained);
}

#[test]
fn rejected_snapshot_is_atomic_guarded_and_cannot_retract_success() {
    use slingshot_domain::{command_fingerprint::{CommandFingerprint, FingerprintInput}, installation::InstallationIdentifier, operation::*};
    use slingshot_storage::{operation_repository::{AdmissionRequest, OperationRepository}, agent_job_repository::FailedAgentSnapshot};
    for (known_success, partial) in [(false, false), (true, false), (false, true), (true, true)] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("rejection.sqlite3");
        let remote = AgentJobRepository::new(OperationDatabase::open(&path, settings()).unwrap());
        let local = OperationRepository::new(OperationDatabase::open(&path, settings()).unwrap());
        let expected = submission_in(&"1d".repeat(32), "rejected");
        let identity = &expected.identity;
        remote.submit(&expected).unwrap();
        local.admit(&AdmissionRequest {
            author_target_identity: "opaque-target".to_owned(),
            author_target_identity_digest: identity.author_target_identity_digest.clone(),
            caller_identity: None,
            canonical_command: "{}".to_owned(),
            command_fingerprint: CommandFingerprint::derive(&FingerprintInput {
                author_target_identity_digest: identity.author_target_identity_digest.clone(),
                canonical_command: "{}".to_owned(), command_wire_name: "query_paths".to_owned(),
                command_semantic_contract_version: "1".to_owned(),
                selected_environment_revision: identity.selected_environment_revision.clone(),
            }).unwrap(),
            command_wire_name: "query_paths".to_owned(),
            daemon_runtime_contract_digest: "c".repeat(64),
            installation_identifier: InstallationIdentifier::parse(&"a1".repeat(32)).unwrap(),
            operation_identifier: identity.operation_identifier.clone(),
            selected_environment_revision: identity.selected_environment_revision.clone(),
            workflow_correlation_identifier: None,
        }, NOW).unwrap();
        let owner = local.apply(&identity.author_target_identity_digest, &identity.operation_identifier, 1,
            &OperationFact::Recovery { recovery: RecoveryFact {
                attempt_count: 0, category: if known_success { RecoveryCategory::ResultAcquisition } else { RecoveryCategory::OperationLookup }, detail: "pending".to_owned(),
                evidence: if known_success { RecoveryExecutionEvidence::AuthoritativeRemoteSuccess }
                else { RecoveryExecutionEvidence::ExecutionCertainty { certainty: OperationExecutionCertainty::RemoteOutcomeUnknown } },
                manual_resume_eligible: false, retry_delay_milliseconds: 0, retry_observed_at_unix_milliseconds: NOW,
            } }, NOW).unwrap();
        let snapshot = FailedAgentSnapshot {
            observation: RemoteJobObservation { state: AgentJobState::Failed, applied_sequence: JobEventSequence::of(3), attempt: 1, progress: 10 },
            physical_sling_job_identifiers: vec!["job-a".to_owned()], remaining_retention_milliseconds: RETENTION,
        };
        let settle = |child: &AgentSubmission, rev, snapshot: &FailedAgentSnapshot| if partial {
            local.settle_partial_admission_snapshot(child, rev, snapshot, NOW + 1)
        } else { local.settle_rejected_agent_snapshot(child, rev, snapshot, None, NOW + 1) };
        assert!(settle(&expected, 1, &snapshot).is_err());
        let mut wrong = expected.clone();
        wrong.contracts.submitted_command_digest = "wrong".to_owned();
        assert!(settle(&wrong, 2, &snapshot).is_err());
        let mut invalid = snapshot.clone();
        invalid.observation.state = AgentJobState::Succeeded;
        assert!(settle(&expected, 2, &invalid).is_err());
        invalid = snapshot.clone();
        invalid.physical_sling_job_identifiers.clear();
        assert!(settle(&expected, 2, &invalid).is_err());
        let external = rusqlite::Connection::open(&path).unwrap();
        external.execute_batch("CREATE TRIGGER refuse_remote_rejection BEFORE UPDATE ON agent_operation BEGIN SELECT RAISE(ABORT, 'injected'); END;").unwrap();
        assert!(settle(&expected, 2, &snapshot).is_err());
        assert_eq!(local.read(&identity.author_target_identity_digest, &identity.operation_identifier).unwrap(), Some(owner.clone()));
        assert_eq!(remote.read(&identity.author_target_identity_digest, &identity.agent_operation_identifier).unwrap(), Some(expected.clone()));
        assert!(remote.physical_jobs(&identity.author_target_identity_digest, &identity.agent_operation_identifier).unwrap().is_empty());
        external.execute_batch("DROP TRIGGER refuse_remote_rejection;").unwrap();
        if known_success {
            assert!(settle(&expected, 2, &snapshot).is_err());
            assert_eq!(local.read(&identity.author_target_identity_digest, &identity.operation_identifier).unwrap(), Some(owner));
        } else {
            let settled = settle(&expected, 2, &snapshot).unwrap();
            assert_eq!(settled.record.terminal_failure.unwrap(), TerminalFailure {
                kind: if partial { TerminalFailureKind::RemoteFailed } else { TerminalFailureKind::Rejected },
                disposition: if partial { TerminalFailureDisposition::AuthoritativeRemoteFailure } else { TerminalFailureDisposition::AuthoritativeNonExecution { certainty: OperationExecutionCertainty::ConfirmedNotExecuted } }, metadata: None,
            });
            let reopened = AgentJobRepository::new(OperationDatabase::open_live(&path, settings()).unwrap());
            let child = reopened.read(&identity.author_target_identity_digest, &identity.agent_operation_identifier).unwrap().unwrap();
            assert_eq!(child.observation, snapshot.observation);
            assert_eq!(child.snapshot_watermark, snapshot.observation.applied_sequence);
            assert_eq!(child.terminal_disposition.as_deref(), Some(if partial { "authoritative-remote-failure" } else { "authoritative-nonexecution" }));
            assert_eq!(reopened.physical_jobs(&identity.author_target_identity_digest, &identity.agent_operation_identifier).unwrap(), snapshot.physical_sling_job_identifiers);
            assert!(settle(&expected, 2, &snapshot).is_err());
        }
    }
}

#[test]
fn snapshot_and_success_transactions_guard_the_owner_and_roll_back_failed_publication() {
    use slingshot_domain::command_fingerprint::{CommandFingerprint, FingerprintInput};
    use slingshot_domain::installation::InstallationIdentifier;
    use slingshot_domain::operation::{
        OperationExecutionCertainty, OperationFact, RecoveryCategory, RecoveryExecutionEvidence,
        RecoveryFact,
    };
    use slingshot_storage::operation_repository::{AdmissionRequest, OperationRepository};
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("guarded.sqlite3");
    let remote = AgentJobRepository::new(OperationDatabase::open(&path, settings()).unwrap());
    let local = OperationRepository::new(OperationDatabase::open(&path, settings()).unwrap());
    let expected = submission_in(&"1d".repeat(32), "guarded");
    let identity = &expected.identity;
    remote.submit(&expected).unwrap();
    let reconcile = |revision| {
        remote.reconcile_active_snapshot_for_operation(
            &expected,
            revision,
            &["job-a".to_owned()],
            running(2, 1, 10),
            RETENTION,
            NOW + 1,
        )
    };
    assert!(reconcile(1).is_err(), "missing local operation must refuse");
    let admission = AdmissionRequest {
        author_target_identity: "opaque-target".to_owned(),
        author_target_identity_digest: identity.author_target_identity_digest.clone(),
        caller_identity: None,
        canonical_command: "{}".to_owned(),
        command_fingerprint: CommandFingerprint::derive(&FingerprintInput {
            author_target_identity_digest: identity.author_target_identity_digest.clone(),
            canonical_command: "{}".to_owned(),
            command_wire_name: "query_paths".to_owned(),
            command_semantic_contract_version: "1".to_owned(),
            selected_environment_revision: identity.selected_environment_revision.clone(),
        })
        .unwrap(),
        command_wire_name: "query_paths".to_owned(),
        daemon_runtime_contract_digest: "c".repeat(64),
        installation_identifier: InstallationIdentifier::parse(&"a1".repeat(32)).unwrap(),
        operation_identifier: identity.operation_identifier.clone(),
        selected_environment_revision: identity.selected_environment_revision.clone(),
        workflow_correlation_identifier: None,
    };
    local.admit(&admission, NOW).unwrap();
    local
        .apply(
            &identity.author_target_identity_digest,
            &identity.operation_identifier,
            1,
            &OperationFact::Recovery {
                recovery: RecoveryFact {
                    attempt_count: 1,
                    category: RecoveryCategory::OperationLookup,
                    detail: "retry".to_owned(),
                    evidence: RecoveryExecutionEvidence::ExecutionCertainty {
                        certainty: OperationExecutionCertainty::SubmissionUnknown,
                    },
                    manual_resume_eligible: true,
                    retry_delay_milliseconds: 1,
                    retry_observed_at_unix_milliseconds: NOW,
                },
            },
            NOW,
        )
        .unwrap();
    assert!(reconcile(1).is_err(), "late response must not use a stale local revision");
    assert_eq!(
        remote
            .read(&identity.author_target_identity_digest, &identity.agent_operation_identifier)
            .unwrap()
            .unwrap(),
        expected
    );
    assert!(
        remote
            .physical_jobs(
                &identity.author_target_identity_digest,
                &identity.agent_operation_identifier
            )
            .unwrap()
            .is_empty()
    );
    reconcile(2).unwrap();
    assert_eq!(
        remote
            .read(&identity.author_target_identity_digest, &identity.agent_operation_identifier)
            .unwrap()
            .unwrap()
            .snapshot_watermark,
        JobEventSequence::of(2)
    );
    let retained = remote
        .read(&identity.author_target_identity_digest, &identity.agent_operation_identifier)
        .unwrap()
        .unwrap();
    let receipt = local
        .record_eligible_resume_receipt(
            &identity.author_target_identity_digest,
            &identity.operation_identifier,
            "activation-fixture",
            &identity.selected_environment_revision,
            RecoveryCategory::OperationLookup,
            2,
            NOW + 1,
        )
        .unwrap();
    let slingshot_storage::operation_repository::ResumeOutcome::Applied(receipt) = receipt else {
        panic!("fresh receipt");
    };
    let activation_fault = rusqlite::Connection::open(&path).unwrap();
    activation_fault.execute_batch("CREATE TRIGGER refuse_activation BEFORE UPDATE ON operation BEGIN SELECT RAISE(ABORT, 'activation fault'); END;").unwrap();
    assert!(
        local
            .activate_retained_recovery(
                &retained,
                &receipt,
                RecoveryCategory::OperationLookup,
                NOW + 2
            )
            .is_err()
    );
    let paused = local
        .read(&identity.author_target_identity_digest, &identity.operation_identifier)
        .unwrap()
        .unwrap();
    assert_eq!(paused.record.revision, 2);
    assert!(paused.record.outstanding_recovery.unwrap().manual_resume_eligible);
    activation_fault.execute_batch("DROP TRIGGER refuse_activation;").unwrap();
    let activated = local
        .activate_retained_recovery(&retained, &receipt, RecoveryCategory::OperationLookup, NOW + 2)
        .unwrap()
        .unwrap();
    assert_eq!(activated.record.revision, 3);
    assert!(!activated.record.outstanding_recovery.unwrap().manual_resume_eligible);
    let reopened_activation =
        OperationRepository::new(OperationDatabase::open_live(&path, settings()).unwrap());
    assert!(
        reopened_activation
            .activate_retained_recovery(
                &retained,
                &receipt,
                RecoveryCategory::OperationLookup,
                NOW + 2
            )
            .unwrap()
            .is_none()
    );
    let owner = local
        .read(&identity.author_target_identity_digest, &identity.operation_identifier)
        .unwrap()
        .unwrap();
    let mut settlement = slingshot_domain::operation::SuccessfulSettlement {
        artifacts: vec![slingshot_domain::operation::ProducedArtifact {
            artifact_identifier: "a".repeat(64),
            artifact_slot: "structured_result".to_owned(),
            byte_length: 2,
            content_digest: "b".repeat(64),
            media_type: "application/json".to_owned(),
        }],
        inline_result: None,
        expected_lifecycle_state: owner.record.lifecycle_state,
        expected_revision: owner.record.revision,
        settled_at_unix_milliseconds: NOW + 2,
    };
    let external = rusqlite::Connection::open(&path).unwrap();
    let snapshot = slingshot_storage::agent_job_repository::SuccessfulAgentSnapshot {
        observation: RemoteJobObservation {
            state: AgentJobState::Succeeded,
            applied_sequence: JobEventSequence::of(3),
            attempt: 1,
            progress: 100,
        },
        physical_sling_job_identifiers: vec!["job-a".to_owned(), "job-b".to_owned()],
        remaining_retention_milliseconds: RETENTION / 2,
    };
    external.execute_batch("CREATE TRIGGER refuse_result_publication BEFORE UPDATE ON operation BEGIN SELECT RAISE(ABORT, 'injected failure'); END;").unwrap();
    assert!(local.settle_success_for_retained_agent(&retained, &settlement).is_err());
    assert_eq!(
        local
            .read(&identity.author_target_identity_digest, &identity.operation_identifier)
            .unwrap(),
        Some(owner.clone())
    );
    assert_eq!(
        remote
            .read(&identity.author_target_identity_digest, &identity.agent_operation_identifier)
            .unwrap(),
        Some(retained.clone())
    );
    for table in ["artifact_blob", "artifact_association"] {
        let count: i64 = external
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0, "{table}: failed publication must roll back earlier inserts");
    }
    external.execute_batch("DROP TRIGGER refuse_result_publication;").unwrap();
    let mut changed = retained.clone();
    changed.identity.agent_operation_identifier = "absent-child".to_owned();
    assert!(local.settle_success_for_retained_agent(&changed, &settlement).is_err());
    changed = retained.clone();
    changed.contracts.submitted_command_digest = "c".repeat(64);
    assert!(local.settle_success_for_retained_agent(&changed, &settlement).is_err());
    external.execute_batch("CREATE TRIGGER refuse_remote_publication BEFORE UPDATE ON agent_operation BEGIN SELECT RAISE(ABORT, 'injected remote failure'); END;").unwrap();
    assert!(local.settle_success_for_agent_snapshot(&retained, &settlement, &snapshot).is_err());
    assert_eq!(
        local
            .read(&identity.author_target_identity_digest, &identity.operation_identifier)
            .unwrap(),
        Some(owner)
    );
    assert_eq!(
        remote
            .read(&identity.author_target_identity_digest, &identity.agent_operation_identifier)
            .unwrap(),
        Some(retained.clone())
    );
    assert_eq!(
        remote
            .physical_jobs(
                &identity.author_target_identity_digest,
                &identity.agent_operation_identifier
            )
            .unwrap(),
        vec!["job-a".to_owned()]
    );
    for table in ["artifact_blob", "artifact_association"] {
        let count: i64 = external
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0, "remote publication failure rolls back local artifacts");
    }
    external.execute_batch("DROP TRIGGER refuse_remote_publication;").unwrap();
    for invalid in [
        slingshot_storage::agent_job_repository::SuccessfulAgentSnapshot {
            physical_sling_job_identifiers: vec!["job-b".to_owned()],
            ..snapshot.clone()
        },
        slingshot_storage::agent_job_repository::SuccessfulAgentSnapshot {
            observation: running(2, 1, 10),
            ..snapshot.clone()
        },
        slingshot_storage::agent_job_repository::SuccessfulAgentSnapshot {
            observation: RemoteJobObservation {
                applied_sequence: JobEventSequence::of(u64::MAX),
                ..snapshot.observation
            },
            ..snapshot.clone()
        },
    ] {
        assert!(local.settle_success_for_agent_snapshot(&retained, &settlement, &invalid).is_err());
    }
    let publication_database = OperationDatabase::open_live(&path, settings()).unwrap();
    let capacity = slingshot_storage::persistent_capacity::PersistentCapacityAccount::new(
        &publication_database,
        slingshot_domain::persistent_capacity::PersistentCapacityPolicy::embedded(),
    );
    let store =
        slingshot_storage::artifact_store::ArtifactStore::open(&root.path().join("artifacts"))
            .unwrap();
    let digest = <sha2::Sha256 as sha2::Digest>::digest(b"{}")
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let request = slingshot_storage::artifact_store::InstallationRequest {
        artifact_slot: "structured_result".to_owned(),
        author_target_identity_digest: identity.author_target_identity_digest.clone(),
        descriptor: None,
        installation_identifier: local
            .read(&identity.author_target_identity_digest, &identity.operation_identifier)
            .unwrap()
            .unwrap()
            .installation_identifier,
        media_type: "application/json".to_owned(),
        operation_identifier: identity.operation_identifier.clone(),
    };
    let reservation = capacity.reserve_artifact(Some(&digest), 2).unwrap();
    let stage = store.stage_verified(&request, &mut b"{}".as_slice(), 2, &digest).unwrap();
    let publication = capacity.retain_staged_publication(&stage, reservation, NOW).unwrap();
    let other_producer = capacity.retain_staged_publication(&stage, None, NOW).unwrap();
    assert_ne!(publication.identifier(), other_producer.identifier());
    let artifact = stage.publish().unwrap();
    settlement.artifacts[0].artifact_identifier = artifact.artifact_identifier.as_text().to_owned();
    settlement.artifacts[0].content_digest = artifact.content_digest;
    external.execute_batch("CREATE TRIGGER refuse_hold_consumption BEFORE DELETE ON artifact_publication BEGIN SELECT RAISE(ABORT, 'injected final write failure'); END;").unwrap();
    assert!(
        local
            .settle_success_for_publications(
                &retained,
                &settlement,
                &snapshot,
                &[publication.clone()]
            )
            .is_err()
    );
    assert_eq!(capacity.pending_publications().unwrap(), 2);
    assert_eq!(
        remote
            .read(&identity.author_target_identity_digest, &identity.agent_operation_identifier)
            .unwrap(),
        Some(retained.clone())
    );
    assert!(
        !local
            .read(&identity.author_target_identity_digest, &identity.operation_identifier)
            .unwrap()
            .unwrap()
            .record
            .lifecycle_state
            .is_terminal()
    );
    let associations: i64 = external
        .query_row("SELECT COUNT(*) FROM artifact_association", [], |row| row.get(0))
        .unwrap();
    assert_eq!(associations, 0);
    external.execute_batch("DROP TRIGGER refuse_hold_consumption;").unwrap();
    // Complete verified local bytes remain usable even if the author lifetime
    // expired during transfer; do not fabricate one remaining millisecond.
    let snapshot = slingshot_storage::agent_job_repository::SuccessfulAgentSnapshot {
        remaining_retention_milliseconds: 0,
        ..snapshot
    };
    let succeeded = local
        .settle_success_for_publications(&retained, &settlement, &snapshot, &[publication])
        .unwrap();
    assert_eq!(capacity.pending_publications().unwrap(), 1, "another producer's hold must survive");
    let ended = remote
        .read(&identity.author_target_identity_digest, &identity.agent_operation_identifier)
        .unwrap()
        .unwrap();
    assert_eq!(ended.observation, snapshot.observation);
    assert_eq!(ended.snapshot_watermark, snapshot.observation.applied_sequence);
    assert_eq!(ended.terminal_disposition.as_deref(), Some("authoritative-remote-success"));
    assert_eq!(ended.remaining_retention_milliseconds, 0);
    assert_eq!(
        remote
            .physical_jobs(
                &identity.author_target_identity_digest,
                &identity.agent_operation_identifier
            )
            .unwrap(),
        snapshot.physical_sling_job_identifiers
    );
    assert_eq!(
        succeeded.record.lifecycle_state,
        slingshot_domain::operation::OperationLifecycleState::Succeeded
    );
    assert!(succeeded.record.outstanding_recovery.is_none());
    for table in ["artifact_blob", "artifact_association"] {
        let count: i64 = external
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1, "{table}: a fresh publication commits the complete result");
    }
}

#[test]
fn snapshot_watermark_failure_rolls_back_state_physical_set_and_retention() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("snapshot.sqlite3");
    let repository = AgentJobRepository::new(OperationDatabase::open(&path, settings()).unwrap());
    let expected = submission("snapshot-rollback");
    repository.submit(&expected).unwrap();
    let external = rusqlite::Connection::open(&path).unwrap();
    external.execute_batch("CREATE TRIGGER refuse_snapshot BEFORE UPDATE OF snapshot_watermark ON agent_operation BEGIN SELECT RAISE(ABORT, 'injected failure'); END;").unwrap();
    assert!(
        repository
            .reconcile_active_snapshot(
                &expected,
                &["job-a".to_owned()],
                running(2, 1, 10),
                RETENTION / 2,
                NOW + 1
            )
            .is_err()
    );
    let identifier = &expected.identity.agent_operation_identifier;
    assert_eq!(repository.read(TARGET, identifier).unwrap().unwrap(), expected);
    assert!(repository.physical_jobs(TARGET, identifier).unwrap().is_empty());
}

#[test]
fn acknowledgement_retains_the_complete_set_without_refreshing_its_lifetime() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("acknowledgement.sqlite3");
    let repository = AgentJobRepository::new(OperationDatabase::open(&path, settings()).unwrap());
    let mut expected = submission("acknowledged");
    expected.remaining_retention_milliseconds = 0;
    repository.submit(&expected).unwrap();
    let jobs = vec!["job-a".to_owned(), "job-b".to_owned()];
    repository.acknowledge(&expected, &jobs, RETENTION, NOW + 1).unwrap();
    repository.acknowledge(&expected, &jobs, RETENTION * 2, NOW + 2).unwrap();
    drop(repository);
    let repository = AgentJobRepository::new(OperationDatabase::open(&path, settings()).unwrap());
    let identifier = &expected.identity.agent_operation_identifier;
    assert_eq!(repository.physical_jobs(TARGET, identifier).unwrap(), jobs);
    let retained = repository.read(TARGET, identifier).unwrap().unwrap();
    assert_eq!(retained.remaining_retention_milliseconds, RETENTION);
    assert_eq!(retained.request_start_unix_milliseconds, NOW);
    assert_eq!(retained.canonical_submission, expected.canonical_submission);
    let mut wrong = expected.clone();
    wrong.identity.selected_environment_revision = "another-revision".to_owned();
    assert!(repository.acknowledge(&wrong, &["job-c".to_owned()], RETENTION, NOW + 3).is_err());
    assert_eq!(repository.physical_jobs(TARGET, identifier).unwrap(), jobs);
    for invalid in [
        vec![],
        vec!["job-b".to_owned(), "job-a".to_owned()],
        vec!["job-a".to_owned(), "job-a".to_owned()],
    ] {
        assert!(repository.acknowledge(&expected, &invalid, RETENTION, NOW + 3).is_err());
    }
    assert_eq!(repository.read(TARGET, identifier).unwrap().unwrap(), retained);
}

#[test]
fn acknowledgement_rolls_back_prior_inserts_when_a_later_statement_fails() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("acknowledgement.sqlite3");
    let repository = AgentJobRepository::new(OperationDatabase::open(&path, settings()).unwrap());
    let mut expected = submission("rollback");
    expected.remaining_retention_milliseconds = 0;
    repository.submit(&expected).unwrap();
    let external = rusqlite::Connection::open(&path).unwrap();
    external.execute_batch("CREATE TRIGGER refuse_acknowledgement_lifetime BEFORE UPDATE OF remaining_retention_milliseconds ON agent_operation BEGIN SELECT RAISE(ABORT, 'injected failure'); END;").unwrap();
    let jobs = vec!["job-a".to_owned(), "job-b".to_owned()];
    assert!(repository.acknowledge(&expected, &jobs, RETENTION, NOW + 1).is_err());
    let identifier = &expected.identity.agent_operation_identifier;
    assert!(repository.physical_jobs(TARGET, identifier).unwrap().is_empty());
    assert_eq!(repository.read(TARGET, identifier).unwrap().unwrap(), expected);
}

/// Returns the observation one running job with `attempt` and `progress` has.
fn running(sequence: u64, attempt: u64, progress: u64) -> RemoteJobObservation {
    RemoteJobObservation {
        applied_sequence: JobEventSequence::of(sequence),
        attempt,
        progress,
        state: AgentJobState::Running,
    }
}

/// Returns one event fact at `cursor`.
fn fact(cursor: &str, digest: &str) -> EventFact {
    fact_in(GENERATION, cursor, digest)
}

/// Returns one event fact in the specified event-store generation.
fn fact_in(generation: u64, cursor: &str, digest: &str) -> EventFact {
    EventFact {
        agent_event_store_generation: generation,
        agent_operation_identifier: None,
        canonical_digest: digest.to_owned(),
        cursor: cursor.to_owned(),
        event_bytes: PAGE_BYTES,
        job_sequence: None,
    }
}

/// Returns the outcome `spelling` names.
fn outcome_named(spelling: &str) -> LedgerOutcome {
    match spelling {
        "advanced" => LedgerOutcome::Advanced,
        "exact-replay" => LedgerOutcome::ExactReplay,
        "stale-cursor-only" => LedgerOutcome::StaleCursorOnly,
        "integrity-conflict" => LedgerOutcome::IntegrityConflict,
        other => panic!("{other} is an outcome this suite does not name"),
    }
}

/// Returns the state `spelling` names.
fn state_named(spelling: &str) -> AgentJobState {
    match spelling {
        "queued" => AgentJobState::Queued,
        "running" => AgentJobState::Running,
        other => panic!("{other} is a state these vectors do not use"),
    }
}

#[test]
fn one_submission_records_every_value_a_resubmission_would_have_to_derive() {
    let root = tempfile::tempdir().expect("a temporary directory");
    let path = root.path().join("operations.sqlite3");
    let written = submission("alpha");
    {
        let repository =
            AgentJobRepository::new(OperationDatabase::open(&path, settings()).expect("opened"));
        assert_eq!(repository.submit(&written).expect("admitted"), SubmissionOutcome::Admitted);
    }
    let repository =
        AgentJobRepository::new(OperationDatabase::open(&path, settings()).expect("reopened"));
    let held = repository
        .read(TARGET, &written.identity.agent_operation_identifier)
        .expect("it reads")
        .expect("it is there");
    assert_eq!(
        held, written,
        "a restart that could not re-derive the contracts would resume under a guessed name"
    );
    assert_eq!(held.contracts.submitted_command_digest, "submitted-digest-alpha");
    assert_eq!(held.observation.state, AgentJobState::Queued);
    assert_eq!(held.terminal_disposition, None);
}

#[test]
fn the_same_submission_again_changes_nothing_and_a_different_one_conflicts() {
    let repository = repository();
    let written = submission("alpha");
    repository.submit(&written).expect("admitted");
    assert_eq!(repository.submit(&written).expect("replayed"), SubmissionOutcome::ExactReplay);
    let mut different = written.clone();
    different.contracts.submitted_command_digest = "submitted-digest-other".to_owned();
    assert!(matches!(repository.submit(&different), Err(AgentRepositoryFailure::Conflicted)));
    let held = repository.read(TARGET, &written.identity.agent_operation_identifier);
    assert_eq!(
        held.expect("it reads").expect("it is there").contracts.submitted_command_digest,
        "submitted-digest-alpha",
        "a conflict changes nothing, which is the only safe answer when two things share a name"
    );
}

#[test]
fn the_same_name_against_another_target_is_other_work() {
    let repository = repository();
    repository.submit(&submission_in(TARGET, "alpha")).expect("admitted here");
    repository.submit(&submission_in(ANOTHER_TARGET, "alpha")).expect("admitted there");
    assert!(repository.read(TARGET, "agent-operation-alpha").expect("reads").is_some());
    assert!(repository.read(ANOTHER_TARGET, "agent-operation-alpha").expect("reads").is_some());
    assert!(
        repository
            .read("target-identity-digest-three", "agent-operation-alpha")
            .expect("reads")
            .is_none(),
        "a partition holds what it holds and answers for nothing else"
    );
}

#[test]
fn several_physical_jobs_carry_one_submission_and_the_same_name_twice_changes_nothing() {
    let repository = repository();
    let identity = identity_in(TARGET, "alpha");
    repository.submit(&submission("alpha")).expect("admitted");
    for job in ["sling-job-beta", "sling-job-alpha", "sling-job-beta"] {
        repository.record_physical_job(&identity, job, NOW).expect("at least once is ordinary");
    }
    assert_eq!(
        repository.physical_jobs(TARGET, &identity.agent_operation_identifier).expect("reads"),
        vec!["sling-job-alpha", "sling-job-beta"],
        "duplicate delivery is handled rather than merely survived, and the answer is sorted"
    );
    assert!(matches!(
        repository.record_physical_job(&identity_in(TARGET, "absent"), "sling-job-gamma", NOW),
        Err(AgentRepositoryFailure::NoSuchSubmission { .. })
    ));
}

#[test]
fn one_submission_accumulates_a_bounded_number_of_physical_records() {
    let repository = repository();
    let identity = identity_in(TARGET, "alpha");
    repository.submit(&submission("alpha")).expect("admitted");
    for position in 0..PHYSICAL_JOBS_PER_SUBMISSION {
        repository
            .record_physical_job(&identity, &format!("sling-job-{position:04}"), NOW)
            .expect("within the bound");
    }
    assert!(
        matches!(
            repository.record_physical_job(&identity, "sling-job-beyond", NOW),
            Err(AgentRepositoryFailure::Exhausted { .. })
        ),
        "an unbounded requeue loop would otherwise grow this table without limit"
    );
}

#[test]
fn a_fold_applies_only_to_the_sequence_it_expected_to_find() {
    let repository = repository();
    let identity = identity_in(TARGET, "alpha");
    repository.submit(&submission("alpha")).expect("admitted");
    for vector in vectors("transitions.jsonl") {
        let name = vector["name"].as_str().expect("a name");
        let held = repository.read(TARGET, &identity.agent_operation_identifier);
        let held = held.expect("reads").expect("it is there");
        assert_eq!(held.observation.state, state_named(vector["from"].as_str().expect("a state")));
        let next = RemoteJobObservation {
            applied_sequence: JobEventSequence::of(held.observation.applied_sequence.value() + 1),
            attempt: vector["attempt"].as_u64().expect("an attempt"),
            progress: vector["progress"].as_u64().expect("a progress"),
            state: state_named(vector["to"].as_str().expect("a state")),
        };
        let applied =
            repository.fold_event(&identity, held.observation.applied_sequence, next).is_ok();
        assert_eq!(
            applied,
            vector["accepted"].as_bool().expect("an expectation"),
            "{name}: a physical requeue is the same work running"
        );
    }
    let stale = repository.fold_event(
        &identity,
        JobEventSequence::first(),
        running(SECOND_SEQUENCE, 1, SOME_PROGRESS),
    );
    assert!(
        matches!(stale, Err(AgentRepositoryFailure::NoSuchSubmission { .. })),
        "two folds racing on one row cannot both succeed"
    );
}

#[test]
fn a_watermark_covers_more_and_never_less() {
    let repository = repository();
    let identity = identity_in(TARGET, "alpha");
    repository.submit(&submission("alpha")).expect("admitted");
    repository
        .record_snapshot_watermark(&identity, JobEventSequence::of(FIFTH_SEQUENCE))
        .expect("a snapshot covers more");
    assert!(
        matches!(
            repository.record_snapshot_watermark(&identity, JobEventSequence::of(SECOND_SEQUENCE)),
            Err(AgentRepositoryFailure::NoSuchSubmission { .. })
        ),
        "a snapshot covering less would make settled events look unsettled again"
    );
    let held = repository.read(TARGET, &identity.agent_operation_identifier);
    assert_eq!(
        held.expect("reads").expect("it is there").snapshot_watermark,
        JobEventSequence::of(FIFTH_SEQUENCE)
    );
}

#[test]
fn an_ending_lands_once_and_a_second_ending_finds_no_row_to_land_on() {
    let repository = repository();
    let identity = identity_in(TARGET, "alpha");
    repository.submit(&submission("alpha")).expect("admitted");
    let ended = RemoteJobObservation {
        applied_sequence: JobEventSequence::of(SECOND_SEQUENCE),
        attempt: 1,
        progress: SOME_PROGRESS,
        state: AgentJobState::Succeeded,
    };
    repository.settle(&identity, ended, RETENTION, "authoritative-remote-success").expect("ends");
    let held = repository.read(TARGET, &identity.agent_operation_identifier);
    let held = held.expect("reads").expect("it is there");
    assert_eq!(held.observation.state, AgentJobState::Succeeded);
    assert_eq!(held.terminal_disposition.as_deref(), Some("authoritative-remote-success"));
    assert!(
        matches!(
            repository.settle(&identity, ended, RETENTION, "authoritative-remote-failure"),
            Err(AgentRepositoryFailure::NoSuchSubmission { .. })
        ),
        "an ending is immutable in the store as well as in the domain"
    );
    assert!(
        matches!(
            repository.fold_event(&identity, JobEventSequence::of(SECOND_SEQUENCE), ended),
            Err(AgentRepositoryFailure::NoSuchSubmission { .. })
        ),
        "and no event moves a row that has ended"
    );
}

#[test]
fn a_position_advances_only_to_a_later_one() {
    for vector in vectors("ledger-vectors.jsonl") {
        let name = vector["name"].as_str().expect("a name");
        let ledger = ledger();
        if let Some(held) = vector["held"].as_str() {
            ledger
                .record_event(TARGET, SUBSCRIPTION, &fact(held, "contents-five"), NOW)
                .expect("the held position is this subscription's");
        }
        let outcome = ledger
            .record_event(
                TARGET,
                SUBSCRIPTION,
                &fact(
                    vector["cursor"].as_str().expect("a cursor"),
                    vector["digest"].as_str().expect("a digest"),
                ),
                NOW,
            )
            .expect("every vector is this subscription's");
        assert_eq!(
            outcome,
            outcome_named(vector["outcome"].as_str().expect("an outcome")),
            "{name}"
        );
        let row = ledger.read_subscription(TARGET, SUBSCRIPTION);
        let row = row.expect("reads").expect("it is there");
        let expected = if matches!(outcome, LedgerOutcome::Advanced) {
            vector["cursor"].as_str()
        } else {
            vector["held"].as_str().or(Some("cursor-0001"))
        };
        assert_eq!(row.cursor.as_deref(), expected, "{name}: only a later position moves it");
    }
}

#[test]
fn an_event_about_work_this_daemon_does_not_hold_still_moves_the_stream() {
    let ledger = ledger();
    let unassociated = fact("cursor-0001", "contents-one");
    assert_eq!(unassociated.agent_operation_identifier, None);
    assert_eq!(
        ledger.record_event(TARGET, SUBSCRIPTION, &unassociated, NOW).expect("it is recorded"),
        LedgerOutcome::Advanced,
        "refusing it would leave the position stuck behind events nothing will ever associate"
    );
    let row = ledger.read_subscription(TARGET, SUBSCRIPTION);
    let row = row.expect("reads").expect("it is there");
    assert_eq!(row.cursor.as_deref(), Some("cursor-0001"));
    assert_eq!(row.event_rows, 1);
    assert_eq!(row.event_bytes, PAGE_BYTES);
    assert_eq!(row.unresolved_incident_count, 0);
}

#[test]
fn repeated_disagreement_about_one_position_consumes_one_incident_slot() {
    let ledger = ledger();
    ledger
        .record_event(TARGET, SUBSCRIPTION, &fact("cursor-0005", "contents-five"), NOW)
        .expect("the first position");
    for _ in 0..REPEATED_REPORTS {
        assert_eq!(
            ledger
                .record_event(TARGET, SUBSCRIPTION, &fact("cursor-0005", "contents-other"), NOW)
                .expect("a conflict is an answer"),
            LedgerOutcome::IntegrityConflict
        );
    }
    let row = ledger.read_subscription(TARGET, SUBSCRIPTION);
    let row = row.expect("reads").expect("it is there");
    assert_eq!(
        row.unresolved_incident_count, 1,
        "charging per report would let an agent exhaust the ledger by repeating itself"
    );
    assert_eq!(row.unresolved_incident.as_deref(), Some("cursor-0005"));
    assert_eq!(
        row.canonical_digest.as_deref(),
        Some("contents-five"),
        "the record keeps what it had"
    );
}

#[test]
fn a_captured_high_water_position_is_the_only_way_out_of_a_disagreement() {
    let ledger = ledger();
    ledger
        .record_event(TARGET, SUBSCRIPTION, &fact("cursor-0005", "contents-five"), NOW)
        .expect("the first position");
    ledger
        .record_event(TARGET, SUBSCRIPTION, &fact("cursor-0005", "contents-other"), NOW)
        .expect("a conflict");
    ledger
        .install_high_water(
            TARGET,
            SUBSCRIPTION,
            GENERATION,
            Some("cursor-0005"),
            Some("cursor-0005"),
            LATER_GENERATION,
            "cursor-0100",
            "contents-high",
        )
        .expect("a reset heals it");
    let row = ledger.read_subscription(TARGET, SUBSCRIPTION);
    let row = row.expect("reads").expect("it is there");
    assert_eq!(row.cursor.as_deref(), Some("cursor-0100"));
    assert_eq!(row.high_water_cursor.as_deref(), Some("cursor-0100"));
    assert_eq!(row.agent_event_store_generation, LATER_GENERATION);
    assert_eq!(row.unresolved_incident, None);
    assert_eq!(row.unresolved_incident_count, 0);
    assert!(matches!(
        ledger.install_high_water(
            TARGET,
            "another-subscription",
            GENERATION,
            None,
            None,
            LATER_GENERATION,
            "c",
            "d",
        ),
        Err(AgentRepositoryFailure::NoSuchSubscription { .. })
    ));
}

#[test]
fn recovery_membership_pages_every_unsettled_generation_and_refuses_ledger_only_reset() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("operations.sqlite3");
    let repository = AgentJobRepository::new(OperationDatabase::open(&path, settings()).unwrap());
    let ledger = AgentSubscriptionLedger::new(OperationDatabase::open(&path, settings()).unwrap());
    ledger.open_subscription(TARGET, SUBSCRIPTION, GENERATION, NOW).unwrap();
    ledger.record_event(TARGET, SUBSCRIPTION, &fact("cursor-0005", "contents-five"), NOW).unwrap();
    ledger.record_event(TARGET, SUBSCRIPTION, &fact("cursor-0005", "contents-other"), NOW).unwrap();
    let empty = ledger.read_recovery_view(TARGET, SUBSCRIPTION).unwrap();
    assert!(empty.members().is_empty());
    assert_eq!(format!("{empty:?}"), "SubscriptionRecoveryView([redacted])");
    let mut expected = Vec::new();
    for number in 0..257 {
        let mut child = submission(&format!("member-{number:04}"));
        if number == 0 {
            child.identity.agent_event_store_generation = GENERATION - 1;
            child.identity.selected_environment_revision = "old-revision".into();
        }
        // Remote terminal observation is not a completed local settlement.
        if number == 1 {
            child.observation = RemoteJobObservation {
                state: AgentJobState::Succeeded, applied_sequence: JobEventSequence::of(3), attempt: 1, progress: 100,
            };
            child.terminal_disposition = Some("authoritative-remote-success".into());
            use slingshot_domain::{command_fingerprint::{CommandFingerprint, FingerprintInput}, installation::InstallationIdentifier};
            use slingshot_storage::operation_repository::{AdmissionRequest, OperationRepository};
            let local = OperationRepository::new(OperationDatabase::open(&path, settings()).unwrap());
            local.admit(&AdmissionRequest {
                author_target_identity: "opaque-target".into(), author_target_identity_digest: TARGET.into(),
                caller_identity: None, canonical_command: "{}".into(),
                command_fingerprint: CommandFingerprint::derive(&FingerprintInput {
                    author_target_identity_digest: TARGET.into(), canonical_command: "{}".into(),
                    command_wire_name: "query_paths".into(), command_semantic_contract_version: "1".into(),
                    selected_environment_revision: child.identity.selected_environment_revision.clone(),
                }).unwrap(),
                command_wire_name: "query_paths".into(), daemon_runtime_contract_digest: "c".repeat(64),
                installation_identifier: InstallationIdentifier::parse(&"a1".repeat(32)).unwrap(),
                operation_identifier: child.identity.operation_identifier.clone(),
                selected_environment_revision: child.identity.selected_environment_revision.clone(), workflow_correlation_identifier: None,
            }, NOW).unwrap();
        }
        let observed = child.clone();
        child.observation = RemoteJobObservation::accepted();
        child.terminal_disposition = None;
        repository.submit(&child).unwrap();
        if let Some(disposition) = &observed.terminal_disposition {
            repository.settle(&observed.identity, observed.observation, RETENTION, disposition).unwrap();
        }
        expected.push(observed);
    }
    let mut elsewhere = submission("other-subscription");
    elsewhere.identity.daemon_subscription_identifier = "other".into();
    repository.submit(&elsewhere).unwrap();
    repository.submit(&submission_in(ANOTHER_TARGET, "other-target")).unwrap();
    let mut ended = submission("settled");
    repository.submit(&ended).unwrap();
    ended.terminal_disposition = Some("complete".into());
    ended.observation = RemoteJobObservation { state: AgentJobState::Succeeded, applied_sequence: JobEventSequence::of(3), attempt: 1, progress: 100 };
    repository.settle(&ended.identity, ended.observation, RETENTION, "complete").unwrap();
    let view = ledger.read_recovery_view(TARGET, SUBSCRIPTION).unwrap();
    assert_eq!(view.members(), expected);
    assert_eq!(view.ledger(), empty.ledger());
    let mut policy = PersistentCapacityPolicy::embedded();
    policy.retained_operation_rows = 256;
    let bounded = AgentSubscriptionLedger::bounded(OperationDatabase::open(&path, settings()).unwrap(), policy);
    assert!(matches!(bounded.read_recovery_view(TARGET, SUBSCRIPTION), Err(AgentRepositoryFailure::Exhausted { allowed: 256, .. })));
    assert!(ledger.read_recovery_view(TARGET, "missing").is_err());
    assert!(matches!(ledger.install_high_water(TARGET, SUBSCRIPTION, GENERATION,
        Some("cursor-0005"), Some("cursor-0005"), LATER_GENERATION, "cursor-0100", "high"), Err(AgentRepositoryFailure::Conflicted)));
    assert_eq!(ledger.read_subscription(TARGET, SUBSCRIPTION).unwrap().unwrap(), *view.ledger());
    let reopened = AgentSubscriptionLedger::new(OperationDatabase::open(&path, settings()).unwrap());
    assert_eq!(reopened.read_recovery_view(TARGET, SUBSCRIPTION).unwrap().members(), expected);
    // A read is not a frozen membership permit: a later child must be observed.
    let later = submission("member-9999"); repository.submit(&later).unwrap(); expected.push(later);
    assert_eq!(reopened.read_recovery_view(TARGET, SUBSCRIPTION).unwrap().members(), expected);
}

#[test]
fn empty_recovery_installs_a_boundary_not_a_fabricated_event_and_survives_reopen() {
    for generation in [GENERATION, LATER_GENERATION] {
        let root = tempfile::tempdir().unwrap(); let path = root.path().join("reset.sqlite3");
        let ledger = AgentSubscriptionLedger::new(OperationDatabase::open(&path, settings()).unwrap());
        ledger.open_subscription(TARGET, SUBSCRIPTION, GENERATION, NOW).unwrap();
        ledger.record_event(TARGET, SUBSCRIPTION, &fact("cursor-0005", "contents-five"), NOW).unwrap();
        ledger.record_event(TARGET, SUBSCRIPTION, &fact("cursor-0005", "different"), NOW).unwrap();
        let view = ledger.read_recovery_view(TARGET, SUBSCRIPTION).unwrap();
        ledger.install_empty_recovery(&view, generation, "cursor-0100").unwrap();
        let row = ledger.read_subscription(TARGET, SUBSCRIPTION).unwrap().unwrap();
        assert_eq!(row.cursor.as_deref(), Some("cursor-0100"));
        assert_eq!(row.high_water_cursor, row.cursor);
        assert_eq!(row.compacted_below_cursor, row.cursor);
        assert_eq!(row.canonical_digest, None);
        assert_eq!(row.unresolved_incident, None);
        assert_eq!((row.event_rows, row.event_bytes, row.unresolved_incident_count), (0, 0, 0));
        assert_eq!(row.agent_event_store_generation, generation);
        assert!(matches!(ledger.install_empty_recovery(&view, generation, "cursor-0101"), Err(AgentRepositoryFailure::SubscriptionMoved)));
        let reopened = AgentSubscriptionLedger::new(OperationDatabase::open(&path, settings()).unwrap());
        assert_eq!(reopened.read_subscription(TARGET, SUBSCRIPTION).unwrap().unwrap(), row);
        for cursor in ["cursor-0001", "cursor-0100"] {
            let mut event = fact(cursor, "not-a-captured-digest"); event.agent_event_store_generation = generation;
            assert_eq!(reopened.record_event(TARGET, SUBSCRIPTION, &event, NOW).unwrap(), LedgerOutcome::StaleCursorOnly);
        }
        assert_eq!(reopened.read_subscription(TARGET, SUBSCRIPTION).unwrap().unwrap(), row);
        let mut event = fact("cursor-0101", "actual-event"); event.agent_event_store_generation = generation;
        assert_eq!(reopened.record_event(TARGET, SUBSCRIPTION, &event, NOW).unwrap(), LedgerOutcome::Advanced);
        assert_eq!(reopened.record_event(TARGET, SUBSCRIPTION, &event, NOW).unwrap(), LedgerOutcome::ExactReplay);
        event.canonical_digest = "different".into();
        assert_eq!(reopened.record_event(TARGET, SUBSCRIPTION, &event, NOW).unwrap(), LedgerOutcome::IntegrityConflict);
    }
}

/// Admits the independently retained local owner needed by a product reset.
fn admit_reset_owner(path: &std::path::Path, child: &AgentSubmission) {
    use slingshot_domain::{command_fingerprint::{CommandFingerprint, FingerprintInput}, installation::InstallationIdentifier};
    use slingshot_storage::operation_repository::{AdmissionRequest, OperationRepository};
    let identity = &child.identity;
    OperationRepository::new(OperationDatabase::open(path, settings()).unwrap()).admit(&AdmissionRequest {
        author_target_identity: "opaque-target".into(), author_target_identity_digest: identity.author_target_identity_digest.clone(),
        caller_identity: None, canonical_command: "{}".into(),
        command_fingerprint: CommandFingerprint::derive(&FingerprintInput {
            author_target_identity_digest: identity.author_target_identity_digest.clone(), canonical_command: "{}".into(),
            command_wire_name: "query_paths".into(), command_semantic_contract_version: "1".into(),
            selected_environment_revision: identity.selected_environment_revision.clone(),
        }).unwrap(), command_wire_name: "query_paths".into(), daemon_runtime_contract_digest: "c".repeat(64),
        installation_identifier: InstallationIdentifier::parse(&"a1".repeat(32)).unwrap(),
        operation_identifier: identity.operation_identifier.clone(), selected_environment_revision: identity.selected_environment_revision.clone(),
        workflow_correlation_identifier: None,
    }, NOW).unwrap();
}

#[test]
fn probe_retry_is_one_atomic_attempt_without_replacing_execution_evidence() {
    use slingshot_domain::operation::{OperationFact, RecoveryFact, RecoveryCategory, RecoveryExecutionEvidence, OperationExecutionCertainty};
    use slingshot_storage::operation_repository::OperationRepository;
    for defect in ["", "success", "submission-unknown", "evidence", "count", "physical", "ledger", "local", "owner", "rollback"] {
        let root = tempfile::tempdir().unwrap(); let path = root.path().join("probe-retry.sqlite3");
        let ledger = AgentSubscriptionLedger::new(OperationDatabase::open(&path, settings()).unwrap());
        let repository = AgentJobRepository::new(OperationDatabase::open(&path, settings()).unwrap());
        let operations = OperationRepository::new(OperationDatabase::open(&path, settings()).unwrap());
        ledger.open_subscription(TARGET, SUBSCRIPTION, GENERATION, NOW).unwrap();
        let child = submission("retry-member"); repository.submit(&child).unwrap(); admit_reset_owner(&path, &child);
        repository.record_physical_job(&child.identity, "job-1", NOW).unwrap();
        let recovery = RecoveryFact {
            attempt_count: 1, category: if defect == "success" {RecoveryCategory::ResultAcquisition} else {RecoveryCategory::OperationLookup},
            detail: "held".into(), evidence: if defect == "success" {RecoveryExecutionEvidence::AuthoritativeRemoteSuccess} else {RecoveryExecutionEvidence::ExecutionCertainty {
                certainty: if defect == "submission-unknown" {OperationExecutionCertainty::SubmissionUnknown} else {OperationExecutionCertainty::RemoteOutcomeUnknown},
            }}, manual_resume_eligible: false, retry_delay_milliseconds: 10, retry_observed_at_unix_milliseconds: NOW,
        };
        operations.apply(TARGET, &child.identity.operation_identifier, 1, &OperationFact::Recovery {recovery: recovery.clone()}, NOW).unwrap();
        let view = ledger.read_recovery_view(TARGET, SUBSCRIPTION).unwrap();
        let mut next = recovery.clone(); next.attempt_count = 2; next.detail = "next".into();
        match defect {
            "evidence" => next.evidence = RecoveryExecutionEvidence::ExecutionCertainty {certainty: OperationExecutionCertainty::SubmissionUnknown},
            "count" => next.attempt_count = 3,
            "physical" => { repository.record_physical_job(&child.identity, "job-2", NOW).unwrap(); },
            "ledger" => { ledger.record_event(TARGET, SUBSCRIPTION, &fact("cursor-0001", "new"), NOW).unwrap(); },
            "local" => { operations.apply(TARGET, &child.identity.operation_identifier, 2, &OperationFact::Recovery {recovery: next.clone()}, NOW).unwrap(); },
            "rollback" => { rusqlite::Connection::open(&path).unwrap().execute_batch("CREATE TRIGGER refuse_probe_retry BEFORE INSERT ON recovery_fact BEGIN SELECT RAISE(ABORT, 'injected recovery write failure'); END;").unwrap(); },
            _ => {},
        }
        let local_before = operations.read(TARGET, &child.identity.operation_identifier).unwrap().unwrap();
        let remote_before = repository.read(TARGET, &child.identity.agent_operation_identifier).unwrap();
        let ledger_before = ledger.read_subscription(TARGET, SUBSCRIPTION).unwrap();
        let another = AgentSubscriptionLedger::new(OperationDatabase::open(&path, settings()).unwrap());
        let result = operations.record_subscription_probe_recovery(if defect == "owner" {&another} else {&ledger}, &view, &child.identity.agent_operation_identifier, 2, next.clone(), NOW);
        let reopened = OperationRepository::new(OperationDatabase::open(&path, settings()).unwrap());
        let after = reopened.read(TARGET, &child.identity.operation_identifier).unwrap().unwrap();
        if ["", "success", "submission-unknown"].contains(&defect) {
            assert!(result.is_ok()); assert_eq!(after.record.revision, 3);
            assert!(!after.record.lifecycle_state.is_terminal());
            assert_eq!(after.record.outstanding_recovery.as_ref(), Some(&next));
            assert!(operations.record_subscription_probe_recovery(&ledger, &view, &child.identity.agent_operation_identifier, 2, next, NOW).is_err());
        } else { assert!(result.is_err(), "{defect}"); assert_eq!(after, local_before); }
        assert_eq!(repository.read(TARGET, &child.identity.agent_operation_identifier).unwrap(), remote_before);
        assert_eq!(ledger.read_subscription(TARGET, SUBSCRIPTION).unwrap(), ledger_before);
    }
}

#[test]
fn unavailable_generation_settlement_rechecks_the_complete_view_atomically() {
    use slingshot_domain::operation::{OperationFact, RecoveryFact, RecoveryCategory,
        RecoveryExecutionEvidence, OperationExecutionCertainty, TerminalFailureKind,
        TerminalFailureDisposition};
    use slingshot_storage::operation_repository::OperationRepository;
    for defect in ["", "submission-unknown", "known-success", "known-nonexecution", "physical", "local", "ledger", "member", "owner", "same-generation", "zero-generation", "revision", "rollback", "remote", "backward-time", "overflow-time"] {
        let root = tempfile::tempdir().unwrap(); let path = root.path().join("generation-loss.sqlite3");
        let ledger = AgentSubscriptionLedger::new(OperationDatabase::open(&path, settings()).unwrap());
        let repository = AgentJobRepository::new(OperationDatabase::open(&path, settings()).unwrap());
        let operations = OperationRepository::new(OperationDatabase::open(&path, settings()).unwrap());
        ledger.open_subscription(TARGET, SUBSCRIPTION, GENERATION, NOW).unwrap();
        let child = submission("lost-member"); repository.submit(&child).unwrap(); admit_reset_owner(&path, &child);
        repository.record_physical_job(&child.identity, "job-1", NOW).unwrap();
        let certainty = if defect == "submission-unknown" {OperationExecutionCertainty::SubmissionUnknown} else if defect == "known-nonexecution" {OperationExecutionCertainty::ConfirmedNotExecuted} else {OperationExecutionCertainty::RemoteOutcomeUnknown};
        let recovery = |detail: &str| OperationFact::Recovery { recovery: RecoveryFact {
            attempt_count: 0, category: if defect == "known-success" {RecoveryCategory::ResultAcquisition} else {RecoveryCategory::OperationLookup},
            detail: detail.into(), evidence: if defect == "known-success" {RecoveryExecutionEvidence::AuthoritativeRemoteSuccess} else {RecoveryExecutionEvidence::ExecutionCertainty {certainty}},
            manual_resume_eligible: false, retry_delay_milliseconds: 0, retry_observed_at_unix_milliseconds: NOW,
        }};
        operations.apply(TARGET, &child.identity.operation_identifier, 1, &recovery("held"), NOW).unwrap();
        let view = ledger.read_recovery_view(TARGET, SUBSCRIPTION).unwrap();
        match defect {
            "physical" => { repository.record_physical_job(&child.identity, "job-2", NOW).unwrap(); },
            "local" => { operations.apply(TARGET, &child.identity.operation_identifier, 2, &recovery("moved"), NOW).unwrap(); },
            "ledger" => { ledger.record_event(TARGET, SUBSCRIPTION, &fact("cursor-0001", "new"), NOW).unwrap(); },
            "member" => { repository.submit(&submission("new-member")).unwrap(); },
            "remote" => { repository.record_snapshot_watermark(&child.identity, JobEventSequence::of(2)).unwrap(); },
            "rollback" => { rusqlite::Connection::open(&path).unwrap().execute_batch("CREATE TRIGGER refuse_generation_loss BEFORE DELETE ON recovery_fact BEGIN SELECT RAISE(ABORT, 'injected recovery deletion failure'); END;").unwrap(); },
            _ => {},
        }
        let before = operations.read(TARGET, &child.identity.operation_identifier).unwrap().unwrap();
        let ledger_before = ledger.read_subscription(TARGET, SUBSCRIPTION).unwrap();
        let remote_before = repository.read(TARGET, &child.identity.agent_operation_identifier).unwrap();
        let physical_before = repository.physical_jobs(TARGET, &child.identity.agent_operation_identifier).unwrap();
        let another = AgentSubscriptionLedger::new(OperationDatabase::open(&path, settings()).unwrap());
        let result = operations.settle_unavailable_generation(
            if defect == "owner" {&another} else {&ledger}, &view, &child.identity.agent_operation_identifier,
            if defect == "same-generation" {GENERATION} else if defect == "zero-generation" {0} else {GENERATION + 1},
            if defect == "revision" {1} else {2}, if defect == "backward-time" {NOW - 1} else if defect == "overflow-time" {u64::MAX} else {NOW},
        );
        let reopened = OperationRepository::new(OperationDatabase::open(&path, settings()).unwrap());
        let after = reopened.read(TARGET, &child.identity.operation_identifier).unwrap().unwrap();
        if defect.is_empty() || defect == "submission-unknown" {
            assert!(result.is_ok()); assert_eq!(after.record.revision, 3);
            let failure = after.record.terminal_failure.as_ref().unwrap();
            assert_eq!(failure.kind, TerminalFailureKind::RemoteStateLost);
            assert_eq!(failure.disposition, TerminalFailureDisposition::FailClosedIndeterminate {certainty});
            assert!(after.record.outstanding_recovery.is_none());
            assert!(operations.settle_unavailable_generation(&ledger, &view, &child.identity.agent_operation_identifier, GENERATION + 1, 2, NOW).is_err());
        } else { assert!(result.is_err(), "{defect}"); assert_eq!(after, before, "{defect}"); }
        assert_eq!(ledger.read_subscription(TARGET, SUBSCRIPTION).unwrap(), ledger_before);
        assert_eq!(repository.read(TARGET, &child.identity.agent_operation_identifier).unwrap(), remote_before);
        assert_eq!(repository.physical_jobs(TARGET, &child.identity.agent_operation_identifier).unwrap(), physical_before);
        if defect.is_empty() || defect == "submission-unknown" {
            let complete = ledger.read_recovery_view(TARGET, SUBSCRIPTION).unwrap();
            assert!(complete.members().is_empty());
            ledger.install_empty_recovery(&complete, GENERATION + 1, "cursor-0100").unwrap();
            let advanced = ledger.read_subscription(TARGET, SUBSCRIPTION).unwrap().unwrap();
            assert_eq!(advanced.agent_event_store_generation, GENERATION + 1);
            assert_eq!(advanced.cursor.as_deref(), Some("cursor-0100"));
            assert_eq!(repository.read(TARGET, &child.identity.agent_operation_identifier).unwrap(), remote_before);
            assert_eq!(repository.physical_jobs(TARGET, &child.identity.agent_operation_identifier).unwrap(), physical_before);
            let early = maintenance::preview(repository.database(), TARGET, NOW, maintenance::maximum_removals()).unwrap();
            assert!(early.agent_removals.is_empty());
            let aged = maintenance::preview(repository.database(), TARGET, NOW + 1, maintenance::maximum_removals()).unwrap();
            assert_eq!(aged.agent_removals.len(), 1);
            assert_eq!(aged.agent_removals[0].terminal_disposition, "remote_state_lost");
            maintenance::apply(repository.database(), &aged, NOW + 2).unwrap();
            assert!(repository.read(TARGET, &child.identity.agent_operation_identifier).unwrap().is_none());
            assert!(repository.physical_jobs(TARGET, &child.identity.agent_operation_identifier).unwrap().is_empty());
            assert!(operations.read(TARGET, &child.identity.operation_identifier).unwrap().is_none());
            assert!(ledger.read_recovery_view(TARGET, SUBSCRIPTION).unwrap().members().is_empty());
        }
    }
}

#[test]
fn completed_event_cursor_rechecks_terminal_owners_and_rolls_back_failed_writes() {
    use slingshot_domain::operation::{OperationFact,TerminalFailure,TerminalFailureKind,TerminalFailureDisposition,OperationExecutionCertainty};
    use slingshot_storage::operation_repository::OperationRepository;
    for defect in ["", "stale", "future", "generation", "physical", "ledger-moved", "physical-moved", "local-moved", "owner", "write", "wrong-subscription"] {
        let root=tempfile::tempdir().unwrap(); let path=root.path().join("completed-event.sqlite3");
        let ledger=AgentSubscriptionLedger::new(OperationDatabase::open(&path,settings()).unwrap());
        let repository=AgentJobRepository::new(OperationDatabase::open(&path,settings()).unwrap());
        let operations=OperationRepository::new(OperationDatabase::open(&path,settings()).unwrap());
        ledger.open_subscription(TARGET,SUBSCRIPTION,GENERATION,NOW).unwrap();
        ledger.open_subscription(TARGET,"another-subscription",GENERATION,NOW).unwrap();
        let child=submission("completed"); repository.submit(&child).unwrap(); admit_reset_owner(&path,&child);
        repository.record_physical_job(&child.identity,"physical-completed",NOW).unwrap();
        repository.settle(&child.identity,RemoteJobObservation {state:AgentJobState::Failed,applied_sequence:JobEventSequence::of(2),attempt:1,progress:0},RETENTION,"authoritative-nonexecution").unwrap();
        operations.apply(TARGET,&child.identity.operation_identifier,1,&OperationFact::Terminal {failure:TerminalFailure {
            kind:TerminalFailureKind::Rejected,disposition:TerminalFailureDisposition::AuthoritativeNonExecution {certainty:OperationExecutionCertainty::ConfirmedNotExecuted},metadata:None,
        }},NOW).unwrap();
        assert!(ledger.read_recovery_view(TARGET,SUBSCRIPTION).unwrap().members().is_empty());
        if defect == "wrong-subscription" {
            assert!(ledger.read_completed_event(TARGET,"another-subscription",&child.identity.agent_operation_identifier).unwrap().is_none()); continue;
        }
        let view=ledger.read_completed_event(TARGET,SUBSCRIPTION,&child.identity.agent_operation_identifier).unwrap().unwrap();
        assert_eq!(format!("{view:?}"),"CompletedEventView([redacted])");
        let mut event=EventFact {agent_event_store_generation:GENERATION,agent_operation_identifier:Some(child.identity.agent_operation_identifier.clone()),canonical_digest:"a".repeat(64),cursor:"cursor-002".into(),event_bytes:256,job_sequence:Some(2)};
        match defect {
            "stale" => event.job_sequence=Some(1), "future" => event.job_sequence=Some(3), "generation" => event.agent_event_store_generation+=1,
            "ledger-moved" => {ledger.record_event(TARGET,SUBSCRIPTION,&fact("cursor-001","old"),NOW).unwrap();},
            "physical-moved" => {repository.record_physical_job(&child.identity,"concurrent",NOW).unwrap();},
            "local-moved" => {rusqlite::Connection::open(&path).unwrap().execute("UPDATE operation SET operation_revision = operation_revision + 1",[]).unwrap();},
            "write" => {rusqlite::Connection::open(&path).unwrap().execute_batch("CREATE TRIGGER refuse_completed_event BEFORE INSERT ON subscription_event BEGIN SELECT RAISE(ABORT, 'injected cursor failure'); END;").unwrap();},
            _ => {},
        }
        let before=ledger.read_subscription(TARGET,SUBSCRIPTION).unwrap().unwrap();
        let local=operations.read(TARGET,&child.identity.operation_identifier).unwrap();
        let remote=repository.read(TARGET,&child.identity.agent_operation_identifier).unwrap();
        let other=AgentSubscriptionLedger::new(OperationDatabase::open(&path,settings()).unwrap());
        let writer=if defect=="owner" {&other} else {&ledger};
        let result=writer.record_completed_event_cursor(&view,&event,if defect=="physical" {"unknown"} else {"physical-completed"},NOW);
        assert_eq!(result.is_ok(),["","stale"].contains(&defect),"{defect}: {result:?}");
        let after=other.read_subscription(TARGET,SUBSCRIPTION).unwrap().unwrap();
        if result.is_ok() {assert_eq!(after.cursor.as_deref(),Some("cursor-002"));assert_eq!(after.event_rows,1);assert_eq!(after.event_bytes,256);}
        else {assert_eq!(after,before);}
        assert_eq!(operations.read(TARGET,&child.identity.operation_identifier).unwrap(),local);
        assert_eq!(repository.read(TARGET,&child.identity.agent_operation_identifier).unwrap(),remote);
    }
}

#[test]
fn event_conflict_is_guarded_durable_and_never_moves_cursor_or_job() {
    for defect in ["", "repeated", "ledger-moved", "physical-moved", "owner", "write", "cursor"] {
        let root = tempfile::tempdir().unwrap(); let path = root.path().join("event-conflict.sqlite3");
        let ledger = AgentSubscriptionLedger::new(OperationDatabase::open(&path, settings()).unwrap());
        let repository = AgentJobRepository::new(OperationDatabase::open(&path, settings()).unwrap());
        ledger.open_subscription(TARGET, SUBSCRIPTION, GENERATION, NOW).unwrap();
        let child = submission("conflict"); repository.submit(&child).unwrap(); admit_reset_owner(&path, &child);
        ledger.record_event(TARGET,SUBSCRIPTION,&fact("cursor-001","first"),NOW).unwrap();
        if defect == "repeated" {
            let view = ledger.read_recovery_view(TARGET,SUBSCRIPTION).unwrap();
            ledger.record_event_conflict(&view,"cursor-002").unwrap();
        }
        let view = ledger.read_recovery_view(TARGET,SUBSCRIPTION).unwrap();
        match defect {
            "ledger-moved" => { ledger.record_event(TARGET,SUBSCRIPTION,&fact("cursor-002","later"),NOW).unwrap(); },
            "physical-moved" => { repository.record_physical_job(&child.identity,"concurrent",NOW).unwrap(); },
            "write" => { rusqlite::Connection::open(&path).unwrap().execute_batch("CREATE TRIGGER refuse_incident BEFORE UPDATE OF unresolved_incident ON subscription_ledger BEGIN SELECT RAISE(ABORT, 'injected incident failure'); END;").unwrap(); },
            _ => {},
        }
        let before = ledger.read_recovery_view(TARGET,SUBSCRIPTION).unwrap();
        let other = AgentSubscriptionLedger::new(OperationDatabase::open(&path,settings()).unwrap());
        let writer = if defect == "owner" {&other} else {&ledger};
        let result = writer.record_event_conflict(&view,if defect == "cursor" {"bad\r\n"} else {"cursor-003"});
        let after = other.read_recovery_view(TARGET,SUBSCRIPTION).unwrap();
        assert_eq!(after.members(),before.members());
        assert_eq!(after.physical_jobs_for(&child.identity.agent_operation_identifier),before.physical_jobs_for(&child.identity.agent_operation_identifier));
        let mut expected = before.ledger().clone();
        if ["","repeated"].contains(&defect) {
            assert!(result.is_ok());
            if defect.is_empty() { expected.unresolved_incident = Some("cursor-003".into()); expected.unresolved_incident_count = 1; }
        } else { assert!(result.is_err(),"{defect}"); }
        assert_eq!(after.ledger(),&expected,"{defect}");
    }
}

#[test]
fn cursor_only_event_rechecks_the_view_and_never_invents_a_job_update() {
    for defect in ["", "associated", "future", "half-key", "half-sequence", "moved", "wrong-owner", "write", "generation", "digest", "overflow"] {
        let root = tempfile::tempdir().unwrap(); let path = root.path().join("cursor-only.sqlite3");
        let ledger = AgentSubscriptionLedger::new(OperationDatabase::open(&path, settings()).unwrap());
        let repository = AgentJobRepository::new(OperationDatabase::open(&path, settings()).unwrap());
        ledger.open_subscription(TARGET, SUBSCRIPTION, GENERATION, NOW).unwrap();
        let child = submission("cursor-only"); repository.submit(&child).unwrap(); admit_reset_owner(&path, &child);
        let view = ledger.read_recovery_view(TARGET, SUBSCRIPTION).unwrap();
        let mut event = EventFact { agent_event_store_generation: GENERATION, agent_operation_identifier: None,
            canonical_digest: "a".repeat(64), cursor: "cursor-002".into(), event_bytes: 256, job_sequence: None };
        match defect {
            "associated" | "future" => { event.agent_operation_identifier = Some(child.identity.agent_operation_identifier.clone()); event.job_sequence = Some(child.observation.applied_sequence.value() + u64::from(defect == "future")); },
            "half-key" => event.agent_operation_identifier = Some(child.identity.agent_operation_identifier.clone()),
            "half-sequence" => event.job_sequence = Some(0),
            "moved" => { ledger.record_event(TARGET, SUBSCRIPTION, &fact("cursor-001", "old"), NOW).unwrap(); },
            "write" => { rusqlite::Connection::open(&path).unwrap().execute_batch("CREATE TRIGGER refuse_cursor_event BEFORE INSERT ON subscription_event BEGIN SELECT RAISE(ABORT, 'injected event failure'); END;").unwrap(); },
            "generation" => event.agent_event_store_generation += 1,
            "digest" => event.canonical_digest = "A".repeat(64),
            "overflow" => event.event_bytes = u64::MAX,
            _ => {},
        }
        let before = ledger.read_recovery_view(TARGET, SUBSCRIPTION).unwrap();
        let other = AgentSubscriptionLedger::new(OperationDatabase::open(&path, settings()).unwrap());
        let writer = if defect == "wrong-owner" { &other } else { &ledger };
        let result = writer.record_cursor_event(&view, &event, NOW);
        let after = ledger.read_recovery_view(TARGET, SUBSCRIPTION).unwrap();
        assert_eq!(after.members(), before.members());
        assert!(repository.physical_jobs(TARGET, &child.identity.agent_operation_identifier).unwrap().is_empty());
        if ["", "associated"].contains(&defect) {
            assert_eq!(result.unwrap(), LedgerOutcome::Advanced);
            assert_eq!(after.ledger().cursor.as_deref(), Some("cursor-002"));
            assert_eq!(after.ledger().event_bytes, 256);
        } else {
            if defect == "generation" { assert_eq!(result.unwrap(), LedgerOutcome::GenerationMismatch); }
            else { assert!(result.is_err(), "{defect}"); }
            assert_eq!(after.ledger(), before.ledger(), "{defect}");
        }
    }
}

#[test]
fn active_event_commits_cursor_job_and_physical_identity_or_rolls_back_all_three() {
    for defect in ["", "known-physical", "job-write", "physical-write", "event-write",
        "ledger-moved", "physical-moved", "remote-moved", "missing-owner", "wrong-owner",
        "gap", "terminal", "generation", "wrong-job", "wrong-sequence", "bad-digest",
        "bad-physical", "unsafe-cursor", "overflow", "backwards-time", "capacity", "incident", "known-success", "local-moved"] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("atomic-event.sqlite3");
        let ledger = AgentSubscriptionLedger::new(OperationDatabase::open(&path, settings()).unwrap());
        let repository = AgentJobRepository::new(OperationDatabase::open(&path, settings()).unwrap());
        ledger.open_subscription(TARGET, SUBSCRIPTION, GENERATION, NOW).unwrap();
        let child = submission("active-event");
        repository.submit(&child).unwrap();
        if defect != "missing-owner" { admit_reset_owner(&path, &child); }
        let record_success = || {
            use slingshot_domain::operation::{OperationFact, RecoveryFact, RecoveryCategory, RecoveryExecutionEvidence};
            use slingshot_storage::operation_repository::OperationRepository;
            OperationRepository::new(OperationDatabase::open(&path, settings()).unwrap()).apply(
                TARGET, &child.identity.operation_identifier, 1,
                &OperationFact::Recovery { recovery: RecoveryFact {
                    attempt_count: 0, category: RecoveryCategory::ResultAcquisition, detail: "pending".into(),
                    evidence: RecoveryExecutionEvidence::AuthoritativeRemoteSuccess,
                    manual_resume_eligible: false, retry_delay_milliseconds: 0,
                    retry_observed_at_unix_milliseconds: NOW,
                } }, NOW).unwrap();
        };
        if defect == "known-success" { record_success(); }
        if defect == "known-physical" {
            repository.record_physical_job(&child.identity, "physical-event", NOW).unwrap();
        }
        if defect == "capacity" {
            for index in 0..PHYSICAL_JOBS_PER_SUBMISSION {
                repository.record_physical_job(&child.identity, &format!("physical-{index}"), NOW).unwrap();
            }
        }
        if defect == "incident" {
            ledger.record_event(TARGET, SUBSCRIPTION, &fact("cursor-0001", "first"), NOW).unwrap();
            ledger.record_event(TARGET, SUBSCRIPTION, &fact("cursor-0001", "other"), NOW).unwrap();
        }
        let view = ledger.read_recovery_view(TARGET, SUBSCRIPTION).unwrap();
        let mut observation = RemoteJobObservation {
            applied_sequence: JobEventSequence::of(child.observation.applied_sequence.value() + 1),
            attempt: 1, progress: 5, state: AgentJobState::Running,
        };
        let mut event = EventFact {
            agent_event_store_generation: GENERATION,
            agent_operation_identifier: Some(child.identity.agent_operation_identifier.clone()),
            canonical_digest: "a".repeat(64), cursor: "cursor-0002".into(), event_bytes: 256,
            job_sequence: Some(observation.applied_sequence.value()),
        };
        match defect {
            "job-write" => { rusqlite::Connection::open(&path).unwrap().execute_batch("CREATE TRIGGER refuse_event_job BEFORE UPDATE OF applied_sequence ON agent_operation BEGIN SELECT RAISE(ABORT, 'injected job failure'); END;").unwrap(); },
            "physical-write" => { rusqlite::Connection::open(&path).unwrap().execute_batch("CREATE TRIGGER refuse_event_physical BEFORE INSERT ON agent_physical_job BEGIN SELECT RAISE(ABORT, 'injected physical failure'); END;").unwrap(); },
            "event-write" => { rusqlite::Connection::open(&path).unwrap().execute_batch("CREATE TRIGGER refuse_event_row BEFORE INSERT ON subscription_event BEGIN SELECT RAISE(ABORT, 'injected event failure'); END;").unwrap(); },
            "ledger-moved" => { ledger.record_event(TARGET, SUBSCRIPTION, &fact("cursor-0001", "old"), NOW).unwrap(); },
            "physical-moved" => { repository.record_physical_job(&child.identity, "concurrent", NOW).unwrap(); },
            "remote-moved" => { repository.record_snapshot_watermark(&child.identity, JobEventSequence::of(1)).unwrap(); },
            "gap" => { observation.applied_sequence = JobEventSequence::of(100); event.job_sequence = Some(100); },
            "terminal" => observation.state = AgentJobState::Succeeded,
            "generation" => event.agent_event_store_generation += 1,
            "wrong-job" => event.agent_operation_identifier = Some("other".into()),
            "wrong-sequence" => event.job_sequence = None,
            "bad-digest" => event.canonical_digest = "A".repeat(64),
            "unsafe-cursor" => event.cursor = "unsafe\r\n".into(),
            "overflow" => observation.attempt = u64::MAX,
            _ => {},
        }
        if defect == "local-moved" { record_success(); }
        let before = ledger.read_recovery_view(TARGET, SUBSCRIPTION).unwrap();
        let physical_before = repository.physical_jobs(TARGET, &child.identity.agent_operation_identifier).unwrap();
        let other = AgentSubscriptionLedger::new(OperationDatabase::open(&path, settings()).unwrap());
        let writer = if defect == "wrong-owner" { &other } else { &ledger };
        let result = writer.record_active_event(&view, &event, observation,
            if defect == "bad-physical" { "" } else { "physical-event" },
            if defect == "backwards-time" { NOW - 1 } else { NOW + 1 });
        let success = ["", "known-physical"].contains(&defect);
        assert_eq!(result.is_ok(), success, "{defect}: {result:?}");
        let reopened = AgentSubscriptionLedger::new(OperationDatabase::open(&path, settings()).unwrap());
        let after = reopened.read_recovery_view(TARGET, SUBSCRIPTION).unwrap();
        let physical_after = repository.physical_jobs(TARGET, &child.identity.agent_operation_identifier).unwrap();
        if success {
            assert_eq!(result.unwrap(), LedgerOutcome::Advanced);
            assert_eq!(after.ledger().cursor.as_deref(), Some("cursor-0002"));
            assert_eq!(after.ledger().event_rows, 1);
            assert_eq!(after.ledger().event_bytes, 256);
            let mut expected_child = child.clone(); expected_child.observation = observation;
            assert_eq!(after.members(), &[expected_child], "retention and snapshot facts must not change");
            assert_eq!(physical_after, ["physical-event"]);
            assert!(ledger.record_active_event(&view, &event, observation, "physical-event", NOW + 1).is_err());
        } else {
            assert_eq!(after.ledger(), before.ledger(), "{defect}: cursor/accounting rolled back");
            assert_eq!(after.members(), before.members(), "{defect}: job rolled back");
            assert_eq!(physical_after, physical_before, "{defect}: physical association rolled back");
        }
    }
}

#[test]
fn active_reset_commits_every_member_and_boundary_or_rolls_back_every_write() {
    use slingshot_storage::agent_subscription_ledger::ActiveResetSnapshot;
    for defect in ["", "older", "missing", "duplicate", "new-member", "remote-moved", "physical-moved", "local-moved", "delete", "second-write", "generation", "terminal", "zero-retention", "omitted-physical", "prior-generation", "missing-owner", "unsafe-watermark", "overflow", "stale-sequence"] {
        let root = tempfile::tempdir().unwrap(); let path = root.path().join("active-reset.sqlite3");
        let ledger = AgentSubscriptionLedger::new(OperationDatabase::open(&path, settings()).unwrap());
        let repository = AgentJobRepository::new(OperationDatabase::open(&path, settings()).unwrap());
        ledger.open_subscription(TARGET, SUBSCRIPTION, GENERATION, NOW).unwrap();
        ledger.record_event(TARGET, SUBSCRIPTION, &fact("cursor-0005", "old"), NOW).unwrap();
        ledger.record_event(TARGET, SUBSCRIPTION, &fact("cursor-0005", "conflict"), NOW).unwrap();
        let mut snapshots = Vec::new();
        for number in 0..2 {
            let mut child = submission(&format!("member-{number}"));
            if number == 1 && defect == "prior-generation" { child.identity.agent_event_store_generation -= 1; }
            repository.submit(&child).unwrap();
            if number != 1 || defect != "missing-owner" { admit_reset_owner(&path, &child); }
            repository.record_physical_job(&child.identity, &format!("job-{number}-existing"), NOW).unwrap();
            snapshots.push(ActiveResetSnapshot {
                agent_operation_identifier: child.identity.agent_operation_identifier,
                observation: running(3 + number, 1, 10 + number),
                physical_sling_job_identifiers: vec![format!("job-{number}-existing"), format!("job-{number}-new")],
                remaining_retention_milliseconds: RETENTION - 100,
                subscription_watermark: "cursor-0100".into(),
            });
        }
        let view = ledger.read_recovery_view(TARGET, SUBSCRIPTION).unwrap();
        match defect {
            "older" => snapshots[1].subscription_watermark = "cursor-0099".into(),
            "missing" => { snapshots.pop(); },
            "duplicate" => snapshots[1] = snapshots[0].clone(),
            "new-member" => { repository.submit(&submission("member-2")).unwrap(); },
            "remote-moved" => { repository.record_snapshot_watermark(&view.members()[1].identity, JobEventSequence::of(2)).unwrap(); },
            "physical-moved" => { repository.record_physical_job(&view.members()[1].identity, "concurrent-job", NOW).unwrap(); },
            "local-moved" => {
                use slingshot_domain::operation::{OperationFact, RecoveryFact, RecoveryCategory, RecoveryExecutionEvidence, OperationExecutionCertainty};
                use slingshot_storage::operation_repository::OperationRepository;
                let child = &view.members()[1];
                OperationRepository::new(OperationDatabase::open(&path, settings()).unwrap()).apply(TARGET, &child.identity.operation_identifier, 1,
                    &OperationFact::Recovery { recovery: RecoveryFact {
                        attempt_count: 0, category: RecoveryCategory::OperationLookup, detail: "pending".into(),
                        evidence: RecoveryExecutionEvidence::ExecutionCertainty { certainty: OperationExecutionCertainty::RemoteOutcomeUnknown },
                        manual_resume_eligible: false, retry_delay_milliseconds: 0, retry_observed_at_unix_milliseconds: NOW,
                    } }, NOW).unwrap();
            },
            "delete" => { rusqlite::Connection::open(&path).unwrap().execute_batch("CREATE TRIGGER refuse_reset_delete BEFORE DELETE ON subscription_event BEGIN SELECT RAISE(ABORT, 'injected deletion failure'); END;").unwrap(); },
            "second-write" => { rusqlite::Connection::open(&path).unwrap().execute_batch("CREATE TRIGGER refuse_second_watermark BEFORE UPDATE OF snapshot_watermark ON agent_operation WHEN OLD.agent_operation_identifier = 'agent-operation-member-1' BEGIN SELECT RAISE(ABORT, 'injected second snapshot failure'); END;").unwrap(); },
            "terminal" => snapshots[1].observation.state = AgentJobState::Succeeded,
            "zero-retention" => snapshots[1].remaining_retention_milliseconds = 0,
            "omitted-physical" => { snapshots[1].physical_sling_job_identifiers.remove(0); },
            "unsafe-watermark" => snapshots[1].subscription_watermark = "z\r\nunsafe".into(),
            "overflow" => snapshots[1].observation.applied_sequence = JobEventSequence::of(u64::MAX),
            "stale-sequence" => snapshots[1].observation.applied_sequence = JobEventSequence::of(0),
            _ => {},
        }
        let before = ledger.read_recovery_view(TARGET, SUBSCRIPTION).unwrap();
        let before_physical: Vec<_> = before.members().iter().map(|child| repository.physical_jobs(TARGET, &child.identity.agent_operation_identifier).unwrap()).collect();
        let result = ledger.install_active_snapshot_reset(&view, if defect == "generation" {LATER_GENERATION} else {GENERATION}, "cursor-0100", &snapshots, NOW + 100);
        assert_eq!(result.is_ok(), defect.is_empty(), "{defect}: {result:?}");
        let reopened = AgentSubscriptionLedger::new(OperationDatabase::open(&path, settings()).unwrap());
        let after = reopened.read_recovery_view(TARGET, SUBSCRIPTION).unwrap();
        if defect.is_empty() {
            assert_eq!(after.ledger().cursor.as_deref(), Some("cursor-0100"));
            assert_eq!(after.ledger().canonical_digest, None);
            assert_eq!(after.ledger().unresolved_incident, None);
            assert_eq!(after.ledger().event_rows, 0);
            for (child, snapshot) in after.members().iter().zip(&snapshots) {
                assert_eq!(child.observation, snapshot.observation);
                assert_eq!(child.snapshot_watermark, snapshot.observation.applied_sequence);
                assert_eq!(child.remaining_retention_milliseconds, RETENTION - 100);
                assert_eq!(repository.physical_jobs(TARGET, &child.identity.agent_operation_identifier).unwrap(), snapshot.physical_sling_job_identifiers);
            }
        } else {
            assert_eq!(after.members(), before.members(), "{defect}");
            assert_eq!(after.ledger(), before.ledger(), "{defect}");
            for (child, physical) in after.members().iter().zip(&before_physical) {
                assert_eq!(&repository.physical_jobs(TARGET, &child.identity.agent_operation_identifier).unwrap(), physical, "{defect}");
            }
        }
    }
}

#[test]
fn empty_recovery_rechecks_owner_membership_and_rolls_back_cursor_on_delete_failure() {
    for defect in ["owner", "new-member", "delete", "moved", "backward", "unsafe"] {
        let root = tempfile::tempdir().unwrap(); let path = root.path().join("reset.sqlite3");
        let ledger = AgentSubscriptionLedger::new(OperationDatabase::open(&path, settings()).unwrap());
        ledger.open_subscription(TARGET, SUBSCRIPTION, GENERATION, NOW).unwrap();
        ledger.record_event(TARGET, SUBSCRIPTION, &fact("cursor-0005", "contents-five"), NOW).unwrap();
        ledger.record_event(TARGET, SUBSCRIPTION, &fact("cursor-0005", "different"), NOW).unwrap();
        let view = ledger.read_recovery_view(TARGET, SUBSCRIPTION).unwrap();
        if defect == "new-member" {
            AgentJobRepository::new(OperationDatabase::open(&path, settings()).unwrap()).submit(&submission("new")).unwrap();
        }
        if defect == "delete" {
            rusqlite::Connection::open(&path).unwrap().execute_batch("CREATE TRIGGER reject_reset_delete BEFORE DELETE ON subscription_event BEGIN SELECT RAISE(ABORT, 'injected reset deletion failure'); END;").unwrap();
        }
        if defect == "moved" {
            ledger.record_event(TARGET, SUBSCRIPTION, &fact("cursor-0006", "six"), NOW).unwrap();
        }
        let before = ledger.read_subscription(TARGET, SUBSCRIPTION).unwrap().unwrap();
        let another = AgentSubscriptionLedger::new(OperationDatabase::open(&path, settings()).unwrap());
        let receiver = if defect == "owner" { &another } else { &ledger };
        let cursor = match defect { "backward" => "cursor-0001", "unsafe" => "bad\r\ncursor", _ => "cursor-0100" };
        assert!(receiver.install_empty_recovery(&view, GENERATION, cursor).is_err(), "{defect}");
        assert_eq!(ledger.read_subscription(TARGET, SUBSCRIPTION).unwrap().unwrap(), before, "{defect}");
        assert_eq!(another.read_subscription(TARGET, SUBSCRIPTION).unwrap().unwrap(), before, "{defect}");
        // A failed post-update delete must leave the retained event and counters intact.
        if defect == "delete" { assert_eq!(ledger.record_event(TARGET, SUBSCRIPTION, &fact("cursor-0005", "contents-five"), NOW).unwrap(), LedgerOutcome::ExactReplay); }
    }
}

#[test]
fn an_old_generation_cannot_mutate_a_reset_ledger_and_a_new_one_reuses_its_cursor() {
    let ledger = ledger();
    ledger
        .record_event(TARGET, SUBSCRIPTION, &fact("cursor-0005", "contents-old"), NOW)
        .expect("the old stream advances");
    ledger
        .record_event(TARGET, SUBSCRIPTION, &fact("cursor-0005", "contents-conflict"), NOW)
        .expect("the disagreement is recorded");
    ledger
        .install_high_water(
            TARGET,
            SUBSCRIPTION,
            GENERATION,
            Some("cursor-0005"),
            Some("cursor-0005"),
            LATER_GENERATION,
            "cursor-0000",
            "contents-reset",
        )
        .expect("the fenced reset wins");
    assert_eq!(
        ledger
            .record_event(TARGET, SUBSCRIPTION, &fact("cursor-0005", "old-late"), NOW)
            .expect("an old event is classified"),
        LedgerOutcome::GenerationMismatch,
        "an old live stream cannot touch the new ledger"
    );
    assert_eq!(
        ledger
            .record_event(
                TARGET,
                SUBSCRIPTION,
                &fact_in(LATER_GENERATION, "cursor-0005", "contents-new"),
                NOW,
            )
            .expect("the new stream reuses its cursor"),
        LedgerOutcome::Advanced,
        "cursor identity is generation-scoped"
    );
    let held = ledger.read_subscription(TARGET, SUBSCRIPTION).expect("reads").expect("held");
    assert_eq!(held.agent_event_store_generation, LATER_GENERATION);
    assert_eq!(held.cursor.as_deref(), Some("cursor-0005"));
    assert_eq!(held.event_rows, 1, "reset removed the old generation's retained events");
}

#[test]
fn a_reset_cannot_regress_or_replace_a_newer_generation() {
    let ledger = ledger();
    ledger
        .record_event(TARGET, SUBSCRIPTION, &fact("cursor-0005", "contents-old"), NOW)
        .expect("the old stream advances");
    ledger
        .record_event(TARGET, SUBSCRIPTION, &fact("cursor-0005", "contents-conflict"), NOW)
        .expect("the disagreement is recorded");
    ledger
        .install_high_water(
            TARGET,
            SUBSCRIPTION,
            GENERATION,
            Some("cursor-0005"),
            Some("cursor-0005"),
            LATER_GENERATION,
            "cursor-0100",
            "contents-new",
        )
        .expect("the first reset wins");
    assert!(matches!(
        ledger.install_high_water(
            TARGET,
            SUBSCRIPTION,
            GENERATION,
            Some("cursor-0005"),
            Some("cursor-0005"),
            GENERATION,
            "cursor-regressed",
            "contents-regressed",
        ),
        Err(AgentRepositoryFailure::SubscriptionMoved)
    ));
    let held = ledger.read_subscription(TARGET, SUBSCRIPTION).expect("reads").expect("held");
    assert_eq!(held.agent_event_store_generation, LATER_GENERATION);
    assert_eq!(held.cursor.as_deref(), Some("cursor-0100"));
}

#[test]
fn an_oversized_first_event_is_refused_without_advancing_the_ledger() {
    let ledger = ledger();
    let mut oversized = fact("cursor-0001", "contents-one");
    oversized.event_bytes = BYTES_PER_EVENT + 1;
    assert!(matches!(
        ledger.record_event(TARGET, SUBSCRIPTION, &oversized, NOW),
        Err(AgentRepositoryFailure::EventTooLarge { .. })
    ));
    let held = ledger.read_subscription(TARGET, SUBSCRIPTION).expect("reads").expect("held");
    assert_eq!(held.event_rows, 0);
    assert_eq!(held.event_bytes, 0);
    assert!(held.cursor.is_none());
}

#[test]
fn compaction_records_its_floor_and_leaves_the_position_where_it_was() {
    let ledger = ledger();
    for position in 1..=RECORDED_POSITIONS {
        ledger
            .record_event(
                TARGET,
                SUBSCRIPTION,
                &fact(&format!("cursor-{position:04}"), &format!("contents-{position}")),
                NOW,
            )
            .expect("each position is this subscription's");
    }
    let removed = ledger.compact_below(TARGET, SUBSCRIPTION, "cursor-0003").expect("it compacts");
    assert_eq!(removed, RETAINED_POSITIONS);
    let row = ledger.read_subscription(TARGET, SUBSCRIPTION);
    let row = row.expect("reads").expect("it is there");
    assert_eq!(
        row.event_rows, RETAINED_POSITIONS,
        "the measured total is recounted rather than assumed"
    );
    assert_eq!(row.event_bytes, PAGE_BYTES * RETAINED_POSITIONS);
    assert_eq!(row.compacted_below_cursor.as_deref(), Some("cursor-0003"));
    assert_eq!(
        row.cursor.as_deref(),
        Some("cursor-0004"),
        "compaction discards history and never the position a reconnection resumes from"
    );
}

#[test]
fn every_derived_bound_admits_its_exact_count_and_refuses_the_next() {
    for vector in vectors("capacity.jsonl") {
        let name = vector["name"].as_str().expect("a name");
        let retained = vector["retained"].as_u64().expect("a bound");
        let mut policy = PersistentCapacityPolicy::embedded();
        policy.retained_operation_rows = retained;
        let bounds = AgentCapacityBounds::derived_from(policy);
        assert_eq!(bounds.agent_submission_rows, retained, "{name}");
        assert_eq!(bounds.subscription_rows, retained, "{name}");
        let database = migrated();
        if name == "submissions" {
            let repository = AgentJobRepository::bounded(database, policy);
            for position in 0..retained {
                repository.submit(&submission(&format!("{position}"))).expect("within the bound");
            }
            assert!(matches!(
                repository.submit(&submission("beyond")),
                Err(AgentRepositoryFailure::Exhausted { .. })
            ));
        } else {
            let ledger = AgentSubscriptionLedger::bounded(database, policy);
            for position in 0..retained {
                ledger
                    .open_subscription(TARGET, &format!("subscription-{position}"), GENERATION, NOW)
                    .expect("within the bound");
            }
            assert!(matches!(
                ledger.open_subscription(TARGET, "subscription-beyond", GENERATION, NOW),
                Err(AgentRepositoryFailure::Exhausted { .. })
            ));
        }
    }
}

#[test]
fn maintenance_reviews_the_remote_half_and_the_local_half_as_one_list() {
    let repository = repository();
    let identity = identity_in(TARGET, "alpha");
    repository.submit(&submission("alpha")).expect("admitted");
    repository.submit(&submission("beta")).expect("admitted");
    let ended = RemoteJobObservation {
        applied_sequence: JobEventSequence::of(SECOND_SEQUENCE),
        attempt: 1,
        progress: SOME_PROGRESS,
        state: AgentJobState::Succeeded,
    };
    repository.settle(&identity, ended, RETENTION, "authoritative-remote-success").expect("ends");
    let manifest = maintenance::preview(
        repository.database(),
        TARGET,
        NOW + 1,
        maintenance::maximum_removals(),
    )
    .expect("a preview reads");
    assert_eq!(manifest.released_agent_rows(), 1, "only ended work is ever selected");
    assert_eq!(manifest.agent_removals[0].agent_operation_identifier, "agent-operation-alpha");
    assert_eq!(
        manifest.agent_removals[0].terminal_disposition, "authoritative-remote-success",
        "a reviewer who saw only the local half would approve removing correlation nobody showed"
    );
    let before = manifest.digest();
    assert_eq!(
        maintenance::preview(
            repository.database(),
            TARGET,
            NOW + 1,
            maintenance::maximum_removals()
        )
        .expect("it reads again")
        .digest(),
        before,
        "a preview changes nothing, including what it would say next time"
    );
    repository.remove_ended(TARGET, "agent-operation-alpha").expect("it is removable");
    assert!(matches!(
        repository.remove_ended(TARGET, "agent-operation-beta"),
        Err(AgentRepositoryFailure::NoSuchSubmission { .. })
    ));
    assert!(repository.read(TARGET, "agent-operation-beta").expect("reads").is_some());
}

#[test]
fn a_subscription_a_retained_submission_still_names_cannot_be_retired() {
    let root = tempfile::tempdir().expect("a temporary directory");
    let path = root.path().join("operations.sqlite3");
    let repository =
        AgentJobRepository::new(OperationDatabase::open(&path, settings()).expect("opened"));
    let ledger =
        AgentSubscriptionLedger::new(OperationDatabase::open(&path, settings()).expect("opened"));
    ledger.open_subscription(TARGET, SUBSCRIPTION, GENERATION, NOW).expect("one subscription");
    assert_eq!(
        ledger.orphaned_subscriptions(TARGET, maintenance::maximum_removals()).expect("reads"),
        vec![SUBSCRIPTION.to_owned()],
        "a subscription nothing retained names is retirable"
    );

    repository.submit(&submission("alpha")).expect("admitted");
    assert!(
        ledger
            .orphaned_subscriptions(TARGET, maintenance::maximum_removals())
            .expect("reads")
            .is_empty(),
        "a submission naming it is a reason to keep it, whichever submission that is"
    );
    assert!(
        matches!(
            ledger.retire_subscription(TARGET, SUBSCRIPTION),
            Err(AgentRepositoryFailure::NoSuchSubscription { .. })
        ),
        "shared replay truth outlives one submission, so the check is made at removal time"
    );

    let ended = RemoteJobObservation {
        applied_sequence: JobEventSequence::of(SECOND_SEQUENCE),
        attempt: 1,
        progress: SOME_PROGRESS,
        state: AgentJobState::Succeeded,
    };
    repository
        .settle(&identity_in(TARGET, "alpha"), ended, RETENTION, "authoritative-remote-success")
        .expect("it ends");
    repository.remove_ended(TARGET, "agent-operation-alpha").expect("ended work is removable");
    ledger.retire_subscription(TARGET, SUBSCRIPTION).expect("now nothing names it");
    assert_eq!(ledger.read_subscription(TARGET, SUBSCRIPTION).expect("reads"), None);
}

#[test]
fn a_position_belongs_to_the_partition_that_recorded_it() {
    let root = tempfile::tempdir().expect("a temporary directory");
    let path = root.path().join("operations.sqlite3");
    let ledger =
        AgentSubscriptionLedger::new(OperationDatabase::open(&path, settings()).expect("opened"));
    ledger.open_subscription(TARGET, SUBSCRIPTION, GENERATION, NOW).expect("one here");
    ledger.open_subscription(ANOTHER_TARGET, SUBSCRIPTION, GENERATION, NOW).expect("one there");
    ledger
        .record_event(TARGET, SUBSCRIPTION, &fact("cursor-0009", "contents-nine"), NOW)
        .expect("one position here");
    let reopened =
        AgentSubscriptionLedger::new(OperationDatabase::open(&path, settings()).expect("reopened"));
    let here = reopened.read_subscription(TARGET, SUBSCRIPTION);
    assert_eq!(
        here.expect("reads").expect("it is there").cursor.as_deref(),
        Some("cursor-0009"),
        "a position survives being written down, which is the only reason to write it down"
    );
    let there = reopened.read_subscription(ANOTHER_TARGET, SUBSCRIPTION);
    assert_eq!(
        there.expect("reads").expect("it is there").cursor,
        None,
        "one subscription name against two targets is two streams"
    );
}
