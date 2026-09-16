//! Durable snapshots integration checks.

use super::*;

/// Sequence reported after the first running snapshot.
const THIRD_SEQUENCE: u64 = 3;
/// Attempt carried by a later running observation.
const SECOND_ATTEMPT: u64 = 2;
/// Progress carried by the first running snapshot.
const EARLY_PROGRESS: u64 = 10;
/// Progress carried by a later running snapshot.
const LATER_PROGRESS: u64 = 20;
/// Progress carried by the successful snapshot.
const COMPLETED_PROGRESS: u64 = 100;
/// Local revision after entering recovery.
const PAUSED_OPERATION_REVISION: u64 = 2;
/// Offset of the later observation from the fixture clock origin.
const LATER_OBSERVATION_OFFSET: u64 = 2;
/// Scale used to test shorter and longer retention offers.
const RETENTION_CHANGE_FACTOR: u64 = 2;
/// Bytes in the empty-object artifact used by the publication fixture.
const ARTIFACT_BYTES: u64 = b"{}".len() as u64;

#[test]
fn bound_startup_audits_outbox_ownership_before_mutable_recovery() {
    use slingshot_domain::{
        command_fingerprint::{CommandFingerprint, FingerprintInput},
        installation::InstallationIdentifier,
    };
    use slingshot_storage::{
        database::StartupDatabaseBinding,
        operation_repository::{AdmissionRequest, OperationRepository},
    };
    for mode in ["matched", "orphan", "wrong-revision", "terminal-orphan"] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("operations.sqlite3");
        let remote = AgentJobRepository::new(OperationDatabase::open(&path, settings()).unwrap());
        let installation =
            InstallationIdentifier::parse(&"a".repeat(IDENTIFIER_CHARACTERS)).unwrap();
        remote.database().record_installation_identifier(&installation, 1).unwrap();
        let target = "1d".repeat(DIGEST_CHARACTERS / "1d".len());
        let mut child = submission_in(&target, "audit");
        let revision = child.identity.selected_environment_revision.clone();
        if mode == "wrong-revision" {
            child.identity.selected_environment_revision = "foreign".into();
        }
        remote.submit(&child).unwrap();
        if mode == "terminal-orphan" {
            let mut observation = child.observation;
            observation.state = AgentJobState::Succeeded;
            remote.settle(&child.identity, observation, RETENTION, "succeeded").unwrap();
        }
        if matches!(mode, "matched" | "wrong-revision") {
            let local =
                OperationRepository::new(OperationDatabase::open_live(&path, settings()).unwrap());
            local
                .admit(
                    &AdmissionRequest {
                        author_target_identity: "opaque-target".into(),
                        author_target_identity_digest: target.clone(),
                        caller_identity: None,
                        canonical_command: "{}".into(),
                        command_fingerprint: CommandFingerprint::derive(&FingerprintInput {
                            author_target_identity_digest: target.clone(),
                            canonical_command: "{}".into(),
                            command_wire_name: "query_paths".into(),
                            command_semantic_contract_version: "1".into(),
                            selected_environment_revision: revision.clone(),
                        })
                        .unwrap(),
                        command_wire_name: "query_paths".into(),
                        daemon_runtime_contract_digest: "c".repeat(DIGEST_CHARACTERS),
                        installation_identifier: installation.clone(),
                        operation_identifier: child.identity.operation_identifier.clone(),
                        selected_environment_revision: revision.clone(),
                        workflow_correlation_identifier: None,
                    },
                    1,
                )
                .unwrap();
        }
        drop(remote);
        let before = std::fs::read(&path).unwrap();
        let contract = "c".repeat(DIGEST_CHARACTERS);
        let result = OperationDatabase::reopen_bound(
            &path,
            settings(),
            StartupDatabaseBinding {
                installation: &installation,
                target: &target,
                revision: &revision,
                runtime_contract: &contract,
            },
        );
        if matches!(mode, "orphan" | "wrong-revision") {
            assert!(result.is_err(), "{mode} reached mutable startup");
            assert_eq!(std::fs::read(&path).unwrap(), before);
        } else {
            assert!(result.is_ok(), "{mode} was refused");
        }
    }
}

#[test]
fn local_operation_lookup_retains_exact_child_evidence_across_restart() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("operations.sqlite3");
    let expected = submission("restart-local");
    let foreign = submission_in(ANOTHER_TARGET, "restart-local");
    {
        let repository =
            AgentJobRepository::new(OperationDatabase::open(&path, settings()).unwrap());
        assert!(
            repository
                .read_for_local_operation(TARGET, &expected.identity.operation_identifier)
                .unwrap()
                .is_none()
        );
        repository.submit(&expected).unwrap();
        repository.submit(&foreign).unwrap();
    }
    let repository = AgentJobRepository::new(OperationDatabase::open(&path, settings()).unwrap());
    assert_eq!(
        repository
            .read_for_local_operation(TARGET, &expected.identity.operation_identifier)
            .unwrap(),
        Some(expected.clone())
    );
    assert_eq!(
        repository
            .read_for_local_operation(ANOTHER_TARGET, &expected.identity.operation_identifier)
            .unwrap(),
        Some(foreign)
    );
    assert!(repository.read_for_local_operation(TARGET, "absent-local").unwrap().is_none());
    assert_eq!(
        repository.read(TARGET, &expected.identity.agent_operation_identifier).unwrap(),
        Some(expected)
    );
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
    let observation = running(SECOND_SEQUENCE, 1, EARLY_PROGRESS);
    repository
        .reconcile_active_snapshot(&expected, &jobs, observation, RETENTION, NOW + 1)
        .unwrap();
    let identifier = &expected.identity.agent_operation_identifier;
    let retained = repository.read(TARGET, identifier).unwrap().unwrap();
    assert_eq!(retained.observation, observation);
    assert_eq!(retained.snapshot_watermark, JobEventSequence::of(SECOND_SEQUENCE));
    assert_eq!(repository.physical_jobs(TARGET, identifier).unwrap(), jobs);
    assert!(
        repository
            .reconcile_active_snapshot(
                &expected,
                &["job-b".to_owned()],
                running(THIRD_SEQUENCE, SECOND_ATTEMPT, LATER_PROGRESS),
                RETENTION,
                NOW + LATER_OBSERVATION_OFFSET
            )
            .is_err()
    );
    assert_eq!(repository.read(TARGET, identifier).unwrap().unwrap(), retained);
    assert_eq!(repository.physical_jobs(TARGET, identifier).unwrap(), jobs);
    let mut terminal = running(THIRD_SEQUENCE, SECOND_ATTEMPT, LATER_PROGRESS);
    terminal.state = AgentJobState::Succeeded;
    assert!(
        repository
            .reconcile_active_snapshot(
                &retained,
                &jobs,
                terminal,
                RETENTION,
                NOW + LATER_OBSERVATION_OFFSET
            )
            .is_err()
    );
    assert!(
        repository
            .reconcile_active_snapshot(
                &retained,
                &jobs,
                running(2, 1, 11),
                RETENTION,
                NOW + LATER_OBSERVATION_OFFSET
            )
            .is_err()
    );
    assert_eq!(repository.read(TARGET, identifier).unwrap().unwrap(), retained);
}

#[test]
fn rejected_snapshot_is_atomic_guarded_and_cannot_retract_success() {
    use slingshot_domain::{
        command_fingerprint::{CommandFingerprint, FingerprintInput},
        installation::InstallationIdentifier,
        operation::*,
    };
    use slingshot_storage::{
        agent_job_repository::FailedAgentSnapshot,
        operation_repository::{AdmissionRequest, OperationRepository},
    };
    for (known_success, partial) in [(false, false), (true, false), (false, true), (true, true)] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("rejection.sqlite3");
        let remote = AgentJobRepository::new(OperationDatabase::open(&path, settings()).unwrap());
        let local = OperationRepository::new(OperationDatabase::open(&path, settings()).unwrap());
        let expected = submission_in(&"1d".repeat(DIGEST_CHARACTERS / "1d".len()), "rejected");
        let identity = &expected.identity;
        remote.submit(&expected).unwrap();
        local
            .admit(
                &AdmissionRequest {
                    author_target_identity: "opaque-target".to_owned(),
                    author_target_identity_digest: identity.author_target_identity_digest.clone(),
                    caller_identity: None,
                    canonical_command: "{}".to_owned(),
                    command_fingerprint: CommandFingerprint::derive(&FingerprintInput {
                        author_target_identity_digest: identity
                            .author_target_identity_digest
                            .clone(),
                        canonical_command: "{}".to_owned(),
                        command_wire_name: "query_paths".to_owned(),
                        command_semantic_contract_version: "1".to_owned(),
                        selected_environment_revision: identity
                            .selected_environment_revision
                            .clone(),
                    })
                    .unwrap(),
                    command_wire_name: "query_paths".to_owned(),
                    daemon_runtime_contract_digest: "c".repeat(DIGEST_CHARACTERS),
                    installation_identifier: InstallationIdentifier::parse(
                        &"a1".repeat(IDENTIFIER_CHARACTERS / "a1".len()),
                    )
                    .unwrap(),
                    operation_identifier: identity.operation_identifier.clone(),
                    selected_environment_revision: identity.selected_environment_revision.clone(),
                    workflow_correlation_identifier: None,
                },
                NOW,
            )
            .unwrap();
        let owner = local
            .apply(
                &identity.author_target_identity_digest,
                &identity.operation_identifier,
                1,
                &OperationFact::Recovery {
                    recovery: RecoveryFact {
                        attempt_count: 0,
                        category: if known_success {
                            RecoveryCategory::ResultAcquisition
                        } else {
                            RecoveryCategory::OperationLookup
                        },
                        detail: "pending".to_owned(),
                        evidence: if known_success {
                            RecoveryExecutionEvidence::AuthoritativeRemoteSuccess
                        } else {
                            RecoveryExecutionEvidence::ExecutionCertainty {
                                certainty: OperationExecutionCertainty::RemoteOutcomeUnknown,
                            }
                        },
                        manual_resume_eligible: false,
                        retry_delay_milliseconds: 0,
                        retry_observed_at_unix_milliseconds: NOW,
                    },
                },
                NOW,
            )
            .unwrap();
        let snapshot = FailedAgentSnapshot {
            observation: RemoteJobObservation {
                state: AgentJobState::Failed,
                applied_sequence: JobEventSequence::of(THIRD_SEQUENCE),
                attempt: 1,
                progress: EARLY_PROGRESS,
            },
            physical_sling_job_identifiers: vec!["job-a".to_owned()],
            remaining_retention_milliseconds: RETENTION,
        };
        let settle = |child: &AgentSubmission, rev, snapshot: &FailedAgentSnapshot| {
            if partial {
                local.settle_partial_admission_snapshot(child, rev, snapshot, None, NOW + 1)
            } else {
                local.settle_rejected_agent_snapshot(child, rev, snapshot, None, None, NOW + 1)
            }
        };
        assert!(settle(&expected, 1, &snapshot).is_err());
        let mut wrong = expected.clone();
        wrong.contracts.submitted_command_digest = "wrong".to_owned();
        assert!(settle(&wrong, 2, &snapshot).is_err());
        let mut invalid = snapshot.clone();
        invalid.observation.state = AgentJobState::Succeeded;
        assert!(settle(&expected, PAUSED_OPERATION_REVISION, &invalid).is_err());
        invalid = snapshot.clone();
        invalid.physical_sling_job_identifiers.clear();
        assert!(settle(&expected, PAUSED_OPERATION_REVISION, &invalid).is_err());
        let external = rusqlite::Connection::open(&path).unwrap();
        external.execute_batch("CREATE TRIGGER refuse_remote_rejection BEFORE UPDATE ON agent_operation BEGIN SELECT RAISE(ABORT, 'injected'); END;").unwrap();
        assert!(settle(&expected, PAUSED_OPERATION_REVISION, &snapshot).is_err());
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
            Some(expected.clone())
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
        external.execute_batch("DROP TRIGGER refuse_remote_rejection;").unwrap();
        if known_success {
            assert!(settle(&expected, PAUSED_OPERATION_REVISION, &snapshot).is_err());
            assert_eq!(
                local
                    .read(&identity.author_target_identity_digest, &identity.operation_identifier)
                    .unwrap(),
                Some(owner)
            );
        } else {
            let settled = settle(&expected, PAUSED_OPERATION_REVISION, &snapshot).unwrap();
            assert_eq!(
                settled.record.terminal_failure.unwrap(),
                TerminalFailure {
                    kind: if partial {
                        TerminalFailureKind::RemoteFailed
                    } else {
                        TerminalFailureKind::Rejected
                    },
                    disposition: if partial {
                        TerminalFailureDisposition::AuthoritativeRemoteFailure
                    } else {
                        TerminalFailureDisposition::AuthoritativeNonExecution {
                            certainty: OperationExecutionCertainty::ConfirmedNotExecuted,
                        }
                    },
                    metadata: None,
                }
            );
            let reopened =
                AgentJobRepository::new(OperationDatabase::open_live(&path, settings()).unwrap());
            let child = reopened
                .read(&identity.author_target_identity_digest, &identity.agent_operation_identifier)
                .unwrap()
                .unwrap();
            assert_eq!(child.observation, snapshot.observation);
            assert_eq!(child.snapshot_watermark, snapshot.observation.applied_sequence);
            assert_eq!(
                child.terminal_disposition.as_deref(),
                Some(if partial {
                    "authoritative-remote-failure"
                } else {
                    "authoritative-nonexecution"
                })
            );
            assert_eq!(
                reopened
                    .physical_jobs(
                        &identity.author_target_identity_digest,
                        &identity.agent_operation_identifier
                    )
                    .unwrap(),
                snapshot.physical_sling_job_identifiers
            );
            assert!(settle(&expected, PAUSED_OPERATION_REVISION, &snapshot).is_err());
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
    let expected = submission_in(&"1d".repeat(DIGEST_CHARACTERS / "1d".len()), "guarded");
    let identity = &expected.identity;
    remote.submit(&expected).unwrap();
    let reconcile = |revision| {
        remote.reconcile_active_snapshot_for_operation(
            &expected,
            revision,
            &["job-a".to_owned()],
            running(SECOND_SEQUENCE, 1, EARLY_PROGRESS),
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
        daemon_runtime_contract_digest: "c".repeat(DIGEST_CHARACTERS),
        installation_identifier: InstallationIdentifier::parse(
            &"a1".repeat(IDENTIFIER_CHARACTERS / "a1".len()),
        )
        .unwrap(),
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
    reconcile(PAUSED_OPERATION_REVISION).unwrap();
    assert_eq!(
        remote
            .read(&identity.author_target_identity_digest, &identity.agent_operation_identifier)
            .unwrap()
            .unwrap()
            .snapshot_watermark,
        JobEventSequence::of(SECOND_SEQUENCE)
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
            PAUSED_OPERATION_REVISION,
            NOW + 1,
        )
        .unwrap();
    let slingshot_storage::operation_repository::ResumeOutcome::Applied(receipt) = receipt else {
        panic!("fresh receipt");
    };
    activation_guards::assert_refusals_preserve_pause(&local, &retained, &receipt);
    let activation_fault = rusqlite::Connection::open(&path).unwrap();
    activation_fault.execute_batch("CREATE TRIGGER refuse_activation BEFORE UPDATE ON operation BEGIN SELECT RAISE(ABORT, 'activation fault'); END;").unwrap();
    assert!(
        local
            .activate_retained_recovery(
                &retained,
                &receipt,
                RecoveryCategory::OperationLookup,
                NOW + LATER_OBSERVATION_OFFSET
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
        .activate_retained_recovery(
            &retained,
            &receipt,
            RecoveryCategory::OperationLookup,
            receipt.recorded_at_unix_milliseconds,
        )
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
                NOW + LATER_OBSERVATION_OFFSET
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
            artifact_identifier: "a".repeat(DIGEST_CHARACTERS),
            artifact_slot: "structured_result".to_owned(),
            byte_length: ARTIFACT_BYTES,
            content_digest: "b".repeat(DIGEST_CHARACTERS),
            media_type: "application/json".to_owned(),
        }],
        inline_result: None,
        expected_lifecycle_state: owner.record.lifecycle_state,
        expected_revision: owner.record.revision,
        settled_at_unix_milliseconds: NOW + LATER_OBSERVATION_OFFSET,
    };
    let external = rusqlite::Connection::open(&path).unwrap();
    let snapshot = slingshot_storage::agent_job_repository::SuccessfulAgentSnapshot {
        observation: RemoteJobObservation {
            state: AgentJobState::Succeeded,
            applied_sequence: JobEventSequence::of(THIRD_SEQUENCE),
            attempt: 1,
            progress: COMPLETED_PROGRESS,
        },
        physical_sling_job_identifiers: vec!["job-a".to_owned(), "job-b".to_owned()],
        remaining_retention_milliseconds: RETENTION / RETENTION_CHANGE_FACTOR,
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
    changed.contracts.submitted_command_digest = "c".repeat(DIGEST_CHARACTERS);
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
            observation: running(SECOND_SEQUENCE, 1, EARLY_PROGRESS),
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
    let reservation = capacity.reserve_artifact(Some(&digest), ARTIFACT_BYTES).unwrap();
    let stage =
        store.stage_verified(&request, &mut b"{}".as_slice(), ARTIFACT_BYTES, &digest).unwrap();
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
                std::slice::from_ref(&publication)
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
                running(SECOND_SEQUENCE, 1, EARLY_PROGRESS),
                RETENTION / RETENTION_CHANGE_FACTOR,
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
    repository
        .acknowledge(
            &expected,
            &jobs,
            RETENTION * RETENTION_CHANGE_FACTOR,
            NOW + LATER_OBSERVATION_OFFSET,
        )
        .unwrap();
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
