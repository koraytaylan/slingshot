//! Durable recovery integration checks.

use super::*;

/// Sequence carried by a terminal fixture snapshot.
const TERMINAL_SEQUENCE: u64 = 3;
/// Progress carried by a successful fixture snapshot.
const COMPLETED_PROGRESS: u64 = 100;
/// Local revision after installing the held recovery fact.
const HELD_RECOVERY_REVISION: u64 = 2;
/// Attempt immediately following the retained first attempt.
const NEXT_PROBE_ATTEMPT: u32 = 2;
/// Nonconsecutive retry count used by the rejection case.
const SKIPPED_PROBE_ATTEMPT: u32 = 3;
/// Clock offset at which aged generation-loss rows are removed.
const MAINTENANCE_APPLY_OFFSET: u64 = 2;

/// Sequence beyond the retained completed observation.
const COMPLETED_EVENT_FUTURE_SEQUENCE: u64 = 3;

/// Bytes charged for one event in these recovery fixtures.
const FIXTURE_EVENT_BYTES: u64 = 256;
/// Retry delay carried by the held recovery fact.
const PROBE_RETRY_DELAY_MILLISECONDS: u64 = 10;

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
    let page_rows = slingshot_storage::sqlite_statement_inventory::STATEMENTS
        .iter()
        .find(|statement| {
            statement.purpose == "page unsettled subscription members across retained generations"
        })
        .unwrap()
        .maximum_rows;
    for number in 0..page_rows + 1 {
        let mut child = submission(&format!("member-{number:04}"));
        if number == 0 {
            child.identity.agent_event_store_generation = GENERATION - 1;
            child.identity.selected_environment_revision = "old-revision".into();
        }
        // Remote terminal observation is not a completed local settlement.
        if number == 1 {
            child.observation = RemoteJobObservation {
                state: AgentJobState::Succeeded,
                applied_sequence: JobEventSequence::of(TERMINAL_SEQUENCE),
                attempt: 1,
                progress: COMPLETED_PROGRESS,
            };
            child.terminal_disposition = Some("authoritative-remote-success".into());
            use slingshot_domain::{
                command_fingerprint::{CommandFingerprint, FingerprintInput},
                installation::InstallationIdentifier,
            };
            use slingshot_storage::operation_repository::{AdmissionRequest, OperationRepository};
            let local =
                OperationRepository::new(OperationDatabase::open(&path, settings()).unwrap());
            local
                .admit(
                    &AdmissionRequest {
                        author_target_identity: "opaque-target".into(),
                        author_target_identity_digest: TARGET.into(),
                        caller_identity: None,
                        canonical_command: "{}".into(),
                        command_fingerprint: CommandFingerprint::derive(&FingerprintInput {
                            author_target_identity_digest: TARGET.into(),
                            canonical_command: "{}".into(),
                            command_wire_name: "query_paths".into(),
                            command_semantic_contract_version: "1".into(),
                            selected_environment_revision: child
                                .identity
                                .selected_environment_revision
                                .clone(),
                        })
                        .unwrap(),
                        command_wire_name: "query_paths".into(),
                        daemon_runtime_contract_digest: "c".repeat(DIGEST_CHARACTERS),
                        installation_identifier: InstallationIdentifier::parse(
                            &"a1".repeat(IDENTIFIER_CHARACTERS / "a1".len()),
                        )
                        .unwrap(),
                        operation_identifier: child.identity.operation_identifier.clone(),
                        selected_environment_revision: child
                            .identity
                            .selected_environment_revision
                            .clone(),
                        workflow_correlation_identifier: None,
                    },
                    NOW,
                )
                .unwrap();
        }
        let observed = child.clone();
        child.observation = RemoteJobObservation::accepted();
        child.terminal_disposition = None;
        repository.submit(&child).unwrap();
        if let Some(disposition) = &observed.terminal_disposition {
            repository
                .settle(&observed.identity, observed.observation, RETENTION, disposition)
                .unwrap();
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
    ended.observation = RemoteJobObservation {
        state: AgentJobState::Succeeded,
        applied_sequence: JobEventSequence::of(TERMINAL_SEQUENCE),
        attempt: 1,
        progress: COMPLETED_PROGRESS,
    };
    repository.settle(&ended.identity, ended.observation, RETENTION, "complete").unwrap();
    let view = ledger.read_recovery_view(TARGET, SUBSCRIPTION).unwrap();
    assert_eq!(view.members(), expected);
    assert_eq!(view.ledger(), empty.ledger());
    let mut policy = PersistentCapacityPolicy::embedded();
    policy.retained_operation_rows = page_rows;
    let bounded = AgentSubscriptionLedger::bounded(
        OperationDatabase::open(&path, settings()).unwrap(),
        policy,
    );
    assert!(matches!(
        bounded.read_recovery_view(TARGET, SUBSCRIPTION),
        Err(AgentRepositoryFailure::Exhausted { allowed, .. }) if allowed == page_rows
    ));
    assert!(ledger.read_recovery_view(TARGET, "missing").is_err());
    assert!(matches!(
        ledger.install_high_water(
            TARGET,
            SUBSCRIPTION,
            GENERATION,
            Some("cursor-0005"),
            Some("cursor-0005"),
            LATER_GENERATION,
            "cursor-0100",
            "high"
        ),
        Err(AgentRepositoryFailure::Conflicted)
    ));
    assert_eq!(ledger.read_subscription(TARGET, SUBSCRIPTION).unwrap().unwrap(), *view.ledger());
    let reopened =
        AgentSubscriptionLedger::new(OperationDatabase::open(&path, settings()).unwrap());
    assert_eq!(reopened.read_recovery_view(TARGET, SUBSCRIPTION).unwrap().members(), expected);
    // A read is not a frozen membership permit: a later child must be observed.
    let later = submission("member-9999");
    repository.submit(&later).unwrap();
    expected.push(later);
    assert_eq!(reopened.read_recovery_view(TARGET, SUBSCRIPTION).unwrap().members(), expected);
}

#[test]
fn empty_recovery_installs_a_boundary_not_a_fabricated_event_and_survives_reopen() {
    for generation in [GENERATION, LATER_GENERATION] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("reset.sqlite3");
        let ledger =
            AgentSubscriptionLedger::new(OperationDatabase::open(&path, settings()).unwrap());
        ledger.open_subscription(TARGET, SUBSCRIPTION, GENERATION, NOW).unwrap();
        ledger
            .record_event(TARGET, SUBSCRIPTION, &fact("cursor-0005", "contents-five"), NOW)
            .unwrap();
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
        assert!(matches!(
            ledger.install_empty_recovery(&view, generation, "cursor-0101"),
            Err(AgentRepositoryFailure::SubscriptionMoved)
        ));
        let reopened =
            AgentSubscriptionLedger::new(OperationDatabase::open(&path, settings()).unwrap());
        assert_eq!(reopened.read_subscription(TARGET, SUBSCRIPTION).unwrap().unwrap(), row);
        for cursor in ["cursor-0001", "cursor-0100"] {
            let mut event = fact(cursor, "not-a-captured-digest");
            event.agent_event_store_generation = generation;
            assert_eq!(
                reopened.record_event(TARGET, SUBSCRIPTION, &event, NOW).unwrap(),
                LedgerOutcome::StaleCursorOnly
            );
        }
        assert_eq!(reopened.read_subscription(TARGET, SUBSCRIPTION).unwrap().unwrap(), row);
        let mut event = fact("cursor-0101", "actual-event");
        event.agent_event_store_generation = generation;
        assert_eq!(
            reopened.record_event(TARGET, SUBSCRIPTION, &event, NOW).unwrap(),
            LedgerOutcome::Advanced
        );
        assert_eq!(
            reopened.record_event(TARGET, SUBSCRIPTION, &event, NOW).unwrap(),
            LedgerOutcome::ExactReplay
        );
        event.canonical_digest = "different".into();
        assert_eq!(
            reopened.record_event(TARGET, SUBSCRIPTION, &event, NOW).unwrap(),
            LedgerOutcome::IntegrityConflict
        );
    }
}

#[test]
fn probe_retry_is_one_atomic_attempt_without_replacing_execution_evidence() {
    use slingshot_domain::operation::{
        OperationExecutionCertainty, OperationFact, RecoveryExecutionEvidence,
    };
    use slingshot_storage::operation_repository::OperationRepository;
    for defect in [
        "",
        "success",
        "submission-unknown",
        "evidence",
        "count",
        "physical",
        "ledger",
        "local",
        "owner",
        "rollback",
    ] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("probe-retry.sqlite3");
        let ledger =
            AgentSubscriptionLedger::new(OperationDatabase::open(&path, settings()).unwrap());
        let repository =
            AgentJobRepository::new(OperationDatabase::open(&path, settings()).unwrap());
        let operations =
            OperationRepository::new(OperationDatabase::open(&path, settings()).unwrap());
        ledger.open_subscription(TARGET, SUBSCRIPTION, GENERATION, NOW).unwrap();
        let child = submission("retry-member");
        repository.submit(&child).unwrap();
        admit_reset_owner(&path, &child);
        repository.record_physical_job(&child.identity, "job-1", NOW).unwrap();
        let recovery = probe_recovery_fact(defect);
        operations
            .apply(
                TARGET,
                &child.identity.operation_identifier,
                1,
                &OperationFact::Recovery { recovery: recovery.clone() },
                NOW,
            )
            .unwrap();
        let view = ledger.read_recovery_view(TARGET, SUBSCRIPTION).unwrap();
        let mut next = recovery.clone();
        next.attempt_count = NEXT_PROBE_ATTEMPT;
        next.detail = "next".into();
        match defect {
            "evidence" => {
                next.evidence = RecoveryExecutionEvidence::ExecutionCertainty {
                    certainty: OperationExecutionCertainty::SubmissionUnknown,
                }
            }
            "count" => next.attempt_count = SKIPPED_PROBE_ATTEMPT,
            "physical" => {
                repository.record_physical_job(&child.identity, "job-2", NOW).unwrap();
            }
            "ledger" => {
                ledger
                    .record_event(TARGET, SUBSCRIPTION, &fact("cursor-0001", "new"), NOW)
                    .unwrap();
            }
            "local" => {
                operations
                    .apply(
                        TARGET,
                        &child.identity.operation_identifier,
                        HELD_RECOVERY_REVISION,
                        &OperationFact::Recovery { recovery: next.clone() },
                        NOW,
                    )
                    .unwrap();
            }
            "rollback" => {
                rusqlite::Connection::open(&path).unwrap().execute_batch("CREATE TRIGGER refuse_probe_retry BEFORE INSERT ON recovery_fact BEGIN SELECT RAISE(ABORT, 'injected recovery write failure'); END;").unwrap();
            }
            _ => {}
        }
        let local_before =
            operations.read(TARGET, &child.identity.operation_identifier).unwrap().unwrap();
        let remote_before =
            repository.read(TARGET, &child.identity.agent_operation_identifier).unwrap();
        let ledger_before = ledger.read_subscription(TARGET, SUBSCRIPTION).unwrap();
        let another =
            AgentSubscriptionLedger::new(OperationDatabase::open(&path, settings()).unwrap());
        let result = operations.record_subscription_probe_recovery(
            if defect == "owner" { &another } else { &ledger },
            &view,
            &child.identity.agent_operation_identifier,
            HELD_RECOVERY_REVISION,
            next.clone(),
            NOW,
        );
        let reopened =
            OperationRepository::new(OperationDatabase::open(&path, settings()).unwrap());
        let after = reopened.read(TARGET, &child.identity.operation_identifier).unwrap().unwrap();
        if ["", "success", "submission-unknown"].contains(&defect) {
            assert!(result.is_ok());
            assert_eq!(after.record.revision, 3);
            assert!(!after.record.lifecycle_state.is_terminal());
            assert_eq!(after.record.outstanding_recovery.as_ref(), Some(&next));
            assert!(
                operations
                    .record_subscription_probe_recovery(
                        &ledger,
                        &view,
                        &child.identity.agent_operation_identifier,
                        HELD_RECOVERY_REVISION,
                        next,
                        NOW
                    )
                    .is_err()
            );
        } else {
            assert!(result.is_err(), "{defect}");
            assert_eq!(after, local_before);
        }
        assert_eq!(
            repository.read(TARGET, &child.identity.agent_operation_identifier).unwrap(),
            remote_before
        );
        assert_eq!(ledger.read_subscription(TARGET, SUBSCRIPTION).unwrap(), ledger_before);
    }
}

#[test]
fn unavailable_generation_settlement_rechecks_the_complete_view_atomically() {
    use slingshot_domain::operation::{TerminalFailureDisposition, TerminalFailureKind};
    use slingshot_storage::operation_repository::OperationRepository;
    for defect in [
        "",
        "submission-unknown",
        "known-success",
        "known-nonexecution",
        "physical",
        "local",
        "ledger",
        "member",
        "owner",
        "same-generation",
        "zero-generation",
        "revision",
        "rollback",
        "remote",
        "backward-time",
        "overflow-time",
    ] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("generation-loss.sqlite3");
        let ledger =
            AgentSubscriptionLedger::new(OperationDatabase::open(&path, settings()).unwrap());
        let repository =
            AgentJobRepository::new(OperationDatabase::open(&path, settings()).unwrap());
        let operations =
            OperationRepository::new(OperationDatabase::open(&path, settings()).unwrap());
        ledger.open_subscription(TARGET, SUBSCRIPTION, GENERATION, NOW).unwrap();
        let child = submission("lost-member");
        repository.submit(&child).unwrap();
        admit_reset_owner(&path, &child);
        repository.record_physical_job(&child.identity, "job-1", NOW).unwrap();
        let certainty = generation_execution_certainty(defect);
        let recovery = |detail: &str| generation_recovery_fact(defect, detail, certainty);
        operations
            .apply(TARGET, &child.identity.operation_identifier, 1, &recovery("held"), NOW)
            .unwrap();
        let view = ledger.read_recovery_view(TARGET, SUBSCRIPTION).unwrap();
        inject_generation_loss_defect(
            defect,
            &path,
            &ledger,
            &repository,
            &operations,
            &child,
            certainty,
        );
        let before =
            operations.read(TARGET, &child.identity.operation_identifier).unwrap().unwrap();
        let ledger_before = ledger.read_subscription(TARGET, SUBSCRIPTION).unwrap();
        let remote_before =
            repository.read(TARGET, &child.identity.agent_operation_identifier).unwrap();
        let physical_before =
            repository.physical_jobs(TARGET, &child.identity.agent_operation_identifier).unwrap();
        let another =
            AgentSubscriptionLedger::new(OperationDatabase::open(&path, settings()).unwrap());
        let (generation, revision, observed_at) = generation_loss_arguments(defect);
        let result = operations.settle_unavailable_generation(
            if defect == "owner" { &another } else { &ledger },
            &view,
            &child.identity.agent_operation_identifier,
            generation,
            revision,
            observed_at,
        );
        let reopened =
            OperationRepository::new(OperationDatabase::open(&path, settings()).unwrap());
        let after = reopened.read(TARGET, &child.identity.operation_identifier).unwrap().unwrap();
        if defect.is_empty() || defect == "submission-unknown" {
            assert!(result.is_ok());
            assert_eq!(after.record.revision, 3);
            let failure = after.record.terminal_failure.as_ref().unwrap();
            assert_eq!(failure.kind, TerminalFailureKind::RemoteStateLost);
            assert_eq!(
                failure.disposition,
                TerminalFailureDisposition::FailClosedIndeterminate { certainty }
            );
            assert!(after.record.outstanding_recovery.is_none());
            assert!(
                operations
                    .settle_unavailable_generation(
                        &ledger,
                        &view,
                        &child.identity.agent_operation_identifier,
                        GENERATION + 1,
                        2,
                        NOW
                    )
                    .is_err()
            );
        } else {
            assert!(result.is_err(), "{defect}");
            assert_eq!(after, before, "{defect}");
        }
        assert_eq!(ledger.read_subscription(TARGET, SUBSCRIPTION).unwrap(), ledger_before);
        assert_eq!(
            repository.read(TARGET, &child.identity.agent_operation_identifier).unwrap(),
            remote_before
        );
        assert_eq!(
            repository.physical_jobs(TARGET, &child.identity.agent_operation_identifier).unwrap(),
            physical_before
        );
        if defect.is_empty() || defect == "submission-unknown" {
            let complete = ledger.read_recovery_view(TARGET, SUBSCRIPTION).unwrap();
            assert!(complete.members().is_empty());
            ledger.install_empty_recovery(&complete, GENERATION + 1, "cursor-0100").unwrap();
            let advanced = ledger.read_subscription(TARGET, SUBSCRIPTION).unwrap().unwrap();
            assert_eq!(advanced.agent_event_store_generation, GENERATION + 1);
            assert_eq!(advanced.cursor.as_deref(), Some("cursor-0100"));
            assert_eq!(
                repository.read(TARGET, &child.identity.agent_operation_identifier).unwrap(),
                remote_before
            );
            assert_eq!(
                repository
                    .physical_jobs(TARGET, &child.identity.agent_operation_identifier)
                    .unwrap(),
                physical_before
            );
            let early = maintenance::preview(
                repository.database(),
                TARGET,
                NOW,
                maintenance::maximum_removals(),
            )
            .unwrap();
            assert!(early.agent_removals.is_empty());
            let aged = maintenance::preview(
                repository.database(),
                TARGET,
                NOW + 1,
                maintenance::maximum_removals(),
            )
            .unwrap();
            assert_eq!(aged.agent_removals.len(), 1);
            assert_eq!(aged.agent_removals[0].terminal_disposition, "remote_state_lost");
            maintenance::apply(repository.database(), &aged, NOW + MAINTENANCE_APPLY_OFFSET)
                .unwrap();
            assert!(
                repository
                    .read(TARGET, &child.identity.agent_operation_identifier)
                    .unwrap()
                    .is_none()
            );
            assert!(
                repository
                    .physical_jobs(TARGET, &child.identity.agent_operation_identifier)
                    .unwrap()
                    .is_empty()
            );
            assert!(
                operations.read(TARGET, &child.identity.operation_identifier).unwrap().is_none()
            );
            assert!(ledger.read_recovery_view(TARGET, SUBSCRIPTION).unwrap().members().is_empty());
        }
    }
}

#[test]
fn completed_event_cursor_rechecks_terminal_owners_and_rolls_back_failed_writes() {
    use slingshot_domain::operation::{
        OperationExecutionCertainty, OperationFact, TerminalFailure, TerminalFailureDisposition,
        TerminalFailureKind,
    };
    use slingshot_storage::operation_repository::OperationRepository;
    for defect in [
        "",
        "stale",
        "future",
        "generation",
        "physical",
        "ledger-moved",
        "physical-moved",
        "local-moved",
        "owner",
        "write",
        "wrong-subscription",
    ] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("completed-event.sqlite3");
        let ledger =
            AgentSubscriptionLedger::new(OperationDatabase::open(&path, settings()).unwrap());
        let repository =
            AgentJobRepository::new(OperationDatabase::open(&path, settings()).unwrap());
        let operations =
            OperationRepository::new(OperationDatabase::open(&path, settings()).unwrap());
        ledger.open_subscription(TARGET, SUBSCRIPTION, GENERATION, NOW).unwrap();
        ledger.open_subscription(TARGET, "another-subscription", GENERATION, NOW).unwrap();
        let child = submission("completed");
        repository.submit(&child).unwrap();
        admit_reset_owner(&path, &child);
        repository.record_physical_job(&child.identity, "physical-completed", NOW).unwrap();
        repository
            .settle(
                &child.identity,
                RemoteJobObservation {
                    state: AgentJobState::Failed,
                    applied_sequence: JobEventSequence::of(SECOND_SEQUENCE),
                    attempt: 1,
                    progress: 0,
                },
                RETENTION,
                "authoritative-nonexecution",
            )
            .unwrap();
        operations
            .apply(
                TARGET,
                &child.identity.operation_identifier,
                1,
                &OperationFact::Terminal {
                    failure: TerminalFailure {
                        kind: TerminalFailureKind::Rejected,
                        disposition: TerminalFailureDisposition::AuthoritativeNonExecution {
                            certainty: OperationExecutionCertainty::ConfirmedNotExecuted,
                        },
                        metadata: None,
                    },
                },
                NOW,
            )
            .unwrap();
        assert!(ledger.read_recovery_view(TARGET, SUBSCRIPTION).unwrap().members().is_empty());
        if defect == "wrong-subscription" {
            assert!(
                ledger
                    .read_completed_event(
                        TARGET,
                        "another-subscription",
                        &child.identity.agent_operation_identifier
                    )
                    .unwrap()
                    .is_none()
            );
            continue;
        }
        let view = ledger
            .read_completed_event(TARGET, SUBSCRIPTION, &child.identity.agent_operation_identifier)
            .unwrap()
            .unwrap();
        assert_eq!(format!("{view:?}"), "CompletedEventView([redacted])");
        let mut event = EventFact {
            agent_event_store_generation: GENERATION,
            agent_operation_identifier: Some(child.identity.agent_operation_identifier.clone()),
            canonical_digest: "a".repeat(DIGEST_CHARACTERS),
            cursor: "cursor-002".into(),
            event_bytes: FIXTURE_EVENT_BYTES,
            job_sequence: Some(SECOND_SEQUENCE),
        };
        inject_completed_event_defect(defect, &path, &ledger, &repository, &child, &mut event);
        let before = ledger.read_subscription(TARGET, SUBSCRIPTION).unwrap().unwrap();
        let local = operations.read(TARGET, &child.identity.operation_identifier).unwrap();
        let remote = repository.read(TARGET, &child.identity.agent_operation_identifier).unwrap();
        let other =
            AgentSubscriptionLedger::new(OperationDatabase::open(&path, settings()).unwrap());
        let writer = if defect == "owner" { &other } else { &ledger };
        let result = writer.record_completed_event_cursor(
            &view,
            &event,
            if defect == "physical" { "unknown" } else { "physical-completed" },
            NOW,
        );
        assert_eq!(result.is_ok(), ["", "stale"].contains(&defect), "{defect}: {result:?}");
        let after = other.read_subscription(TARGET, SUBSCRIPTION).unwrap().unwrap();
        if result.is_ok() {
            assert_eq!(after.cursor.as_deref(), Some("cursor-002"));
            assert_eq!(after.event_rows, 1);
            assert_eq!(after.event_bytes, FIXTURE_EVENT_BYTES);
        } else {
            assert_eq!(after, before);
        }
        assert_eq!(operations.read(TARGET, &child.identity.operation_identifier).unwrap(), local);
        assert_eq!(
            repository.read(TARGET, &child.identity.agent_operation_identifier).unwrap(),
            remote
        );
    }
}

#[test]
fn event_conflict_is_guarded_durable_and_never_moves_cursor_or_job() {
    for defect in ["", "repeated", "ledger-moved", "physical-moved", "owner", "write", "cursor"] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("event-conflict.sqlite3");
        let ledger =
            AgentSubscriptionLedger::new(OperationDatabase::open(&path, settings()).unwrap());
        let repository =
            AgentJobRepository::new(OperationDatabase::open(&path, settings()).unwrap());
        ledger.open_subscription(TARGET, SUBSCRIPTION, GENERATION, NOW).unwrap();
        let child = submission("conflict");
        repository.submit(&child).unwrap();
        admit_reset_owner(&path, &child);
        ledger.record_event(TARGET, SUBSCRIPTION, &fact("cursor-001", "first"), NOW).unwrap();
        if defect == "repeated" {
            let view = ledger.read_recovery_view(TARGET, SUBSCRIPTION).unwrap();
            ledger.record_event_conflict(&view, "cursor-002").unwrap();
        }
        let view = ledger.read_recovery_view(TARGET, SUBSCRIPTION).unwrap();
        match defect {
            "ledger-moved" => {
                ledger
                    .record_event(TARGET, SUBSCRIPTION, &fact("cursor-002", "later"), NOW)
                    .unwrap();
            }
            "physical-moved" => {
                repository.record_physical_job(&child.identity, "concurrent", NOW).unwrap();
            }
            "write" => {
                rusqlite::Connection::open(&path).unwrap().execute_batch("CREATE TRIGGER refuse_incident BEFORE UPDATE OF unresolved_incident ON subscription_ledger BEGIN SELECT RAISE(ABORT, 'injected incident failure'); END;").unwrap();
            }
            _ => {}
        }
        let before = ledger.read_recovery_view(TARGET, SUBSCRIPTION).unwrap();
        let other =
            AgentSubscriptionLedger::new(OperationDatabase::open(&path, settings()).unwrap());
        let writer = if defect == "owner" { &other } else { &ledger };
        let result = writer.record_event_conflict(
            &view,
            if defect == "cursor" { "bad\r\n" } else { "cursor-003" },
        );
        let after = other.read_recovery_view(TARGET, SUBSCRIPTION).unwrap();
        assert_eq!(after.members(), before.members());
        assert_eq!(
            after.physical_jobs_for(&child.identity.agent_operation_identifier),
            before.physical_jobs_for(&child.identity.agent_operation_identifier)
        );
        let mut expected = before.ledger().clone();
        if ["", "repeated"].contains(&defect) {
            assert!(result.is_ok());
            if defect.is_empty() {
                expected.unresolved_incident = Some("cursor-003".into());
                expected.unresolved_incident_count = 1;
            }
        } else {
            assert!(result.is_err(), "{defect}");
        }
        assert_eq!(after.ledger(), &expected, "{defect}");
    }
}

#[test]
fn cursor_only_event_rechecks_the_view_and_never_invents_a_job_update() {
    for defect in [
        "",
        "associated",
        "future",
        "half-key",
        "half-sequence",
        "moved",
        "wrong-owner",
        "write",
        "generation",
        "digest",
        "overflow",
    ] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("cursor-only.sqlite3");
        let ledger =
            AgentSubscriptionLedger::new(OperationDatabase::open(&path, settings()).unwrap());
        let repository =
            AgentJobRepository::new(OperationDatabase::open(&path, settings()).unwrap());
        ledger.open_subscription(TARGET, SUBSCRIPTION, GENERATION, NOW).unwrap();
        let child = submission("cursor-only");
        repository.submit(&child).unwrap();
        admit_reset_owner(&path, &child);
        let view = ledger.read_recovery_view(TARGET, SUBSCRIPTION).unwrap();
        let mut event = EventFact {
            agent_event_store_generation: GENERATION,
            agent_operation_identifier: None,
            canonical_digest: "a".repeat(DIGEST_CHARACTERS),
            cursor: "cursor-002".into(),
            event_bytes: FIXTURE_EVENT_BYTES,
            job_sequence: None,
        };
        inject_cursor_event_defect(defect, &path, &ledger, &child, &mut event);
        let before = ledger.read_recovery_view(TARGET, SUBSCRIPTION).unwrap();
        let other =
            AgentSubscriptionLedger::new(OperationDatabase::open(&path, settings()).unwrap());
        let writer = if defect == "wrong-owner" { &other } else { &ledger };
        let result = writer.record_cursor_event(&view, &event, NOW);
        let after = ledger.read_recovery_view(TARGET, SUBSCRIPTION).unwrap();
        assert_eq!(after.members(), before.members());
        assert!(
            repository
                .physical_jobs(TARGET, &child.identity.agent_operation_identifier)
                .unwrap()
                .is_empty()
        );
        if ["", "associated"].contains(&defect) {
            assert_eq!(result.unwrap(), LedgerOutcome::Advanced);
            assert_eq!(after.ledger().cursor.as_deref(), Some("cursor-002"));
            assert_eq!(after.ledger().event_bytes, FIXTURE_EVENT_BYTES);
        } else {
            if defect == "generation" {
                assert_eq!(result.unwrap(), LedgerOutcome::GenerationMismatch);
            } else {
                assert!(result.is_err(), "{defect}");
            }
            assert_eq!(after.ledger(), before.ledger(), "{defect}");
        }
    }
}

/// Builds the retained evidence before the probe captures its recovery view.
fn probe_recovery_fact(defect: &str) -> slingshot_domain::operation::RecoveryFact {
    use slingshot_domain::operation::{
        OperationExecutionCertainty, RecoveryCategory, RecoveryExecutionEvidence, RecoveryFact,
    };
    RecoveryFact {
        attempt_count: 1,
        category: if defect == "success" {
            RecoveryCategory::ResultAcquisition
        } else {
            RecoveryCategory::OperationLookup
        },
        detail: "held".into(),
        evidence: if defect == "success" {
            RecoveryExecutionEvidence::AuthoritativeRemoteSuccess
        } else {
            RecoveryExecutionEvidence::ExecutionCertainty {
                certainty: if defect == "submission-unknown" {
                    OperationExecutionCertainty::SubmissionUnknown
                } else {
                    OperationExecutionCertainty::RemoteOutcomeUnknown
                },
            }
        },
        manual_resume_eligible: false,
        retry_delay_milliseconds: PROBE_RETRY_DELAY_MILLISECONDS,
        retry_observed_at_unix_milliseconds: NOW,
    }
}

/// Applies the cursor-only case's offered-event or post-capture database defect.
fn inject_cursor_event_defect(
    defect: &str,
    path: &std::path::Path,
    ledger: &AgentSubscriptionLedger,
    child: &AgentSubmission,
    event: &mut EventFact,
) {
    match defect {
        "associated" | "future" => {
            event.agent_operation_identifier =
                Some(child.identity.agent_operation_identifier.clone());
            event.job_sequence =
                Some(child.observation.applied_sequence.value() + u64::from(defect == "future"));
        }
        "half-key" => {
            event.agent_operation_identifier =
                Some(child.identity.agent_operation_identifier.clone())
        }
        "half-sequence" => event.job_sequence = Some(0),
        "moved" => {
            ledger.record_event(TARGET, SUBSCRIPTION, &fact("cursor-001", "old"), NOW).unwrap();
        }
        "write" => {
            rusqlite::Connection::open(path).unwrap().execute_batch("CREATE TRIGGER refuse_cursor_event BEFORE INSERT ON subscription_event BEGIN SELECT RAISE(ABORT, 'injected event failure'); END;").unwrap();
        }
        "generation" => event.agent_event_store_generation += 1,
        "digest" => event.canonical_digest = "A".repeat(DIGEST_CHARACTERS),
        "overflow" => event.event_bytes = u64::MAX,
        _ => {}
    }
}

/// Injects post-capture defects without changing the completed-cursor assertions.
fn inject_completed_event_defect(
    defect: &str,
    path: &std::path::Path,
    ledger: &AgentSubscriptionLedger,
    repository: &AgentJobRepository,
    child: &AgentSubmission,
    event: &mut EventFact,
) {
    match defect {
        "stale" => event.job_sequence = Some(1),
        "future" => event.job_sequence = Some(COMPLETED_EVENT_FUTURE_SEQUENCE),
        "generation" => event.agent_event_store_generation += 1,
        "ledger-moved" => {
            ledger.record_event(TARGET, SUBSCRIPTION, &fact("cursor-001", "old"), NOW).unwrap();
        }
        "physical-moved" => {
            repository.record_physical_job(&child.identity, "concurrent", NOW).unwrap();
        }
        "local-moved" => {
            rusqlite::Connection::open(path)
                .unwrap()
                .execute("UPDATE operation SET operation_revision = operation_revision + 1", [])
                .unwrap();
        }
        "write" => {
            rusqlite::Connection::open(path).unwrap().execute_batch("CREATE TRIGGER refuse_completed_event BEFORE INSERT ON subscription_event BEGIN SELECT RAISE(ABORT, 'injected cursor failure'); END;").unwrap();
        }
        _ => {}
    }
}

/// Retained execution certainty for the generation-loss case.
fn generation_execution_certainty(
    defect: &str,
) -> slingshot_domain::operation::OperationExecutionCertainty {
    use slingshot_domain::operation::OperationExecutionCertainty;
    if defect == "submission-unknown" {
        OperationExecutionCertainty::SubmissionUnknown
    } else if defect == "known-nonexecution" {
        OperationExecutionCertainty::ConfirmedNotExecuted
    } else {
        OperationExecutionCertainty::RemoteOutcomeUnknown
    }
}

/// Builds the held or concurrently updated recovery without changing its evidence.
fn generation_recovery_fact(
    defect: &str,
    detail: &str,
    certainty: slingshot_domain::operation::OperationExecutionCertainty,
) -> slingshot_domain::operation::OperationFact {
    use slingshot_domain::operation::{
        OperationFact, RecoveryCategory, RecoveryExecutionEvidence, RecoveryFact,
    };
    OperationFact::Recovery {
        recovery: RecoveryFact {
            attempt_count: 0,
            category: if defect == "known-success" {
                RecoveryCategory::ResultAcquisition
            } else {
                RecoveryCategory::OperationLookup
            },
            detail: detail.into(),
            evidence: if defect == "known-success" {
                RecoveryExecutionEvidence::AuthoritativeRemoteSuccess
            } else {
                RecoveryExecutionEvidence::ExecutionCertainty { certainty }
            },
            manual_resume_eligible: false,
            retry_delay_milliseconds: 0,
            retry_observed_at_unix_milliseconds: NOW,
        },
    }
}

/// Applies concurrent database changes after the recovery view was captured.
fn inject_generation_loss_defect(
    defect: &str,
    path: &std::path::Path,
    ledger: &AgentSubscriptionLedger,
    repository: &AgentJobRepository,
    operations: &slingshot_storage::operation_repository::OperationRepository,
    child: &AgentSubmission,
    certainty: slingshot_domain::operation::OperationExecutionCertainty,
) {
    match defect {
        "physical" => {
            repository.record_physical_job(&child.identity, "job-2", NOW).unwrap();
        }
        "local" => {
            operations
                .apply(
                    TARGET,
                    &child.identity.operation_identifier,
                    HELD_RECOVERY_REVISION,
                    &generation_recovery_fact(defect, "moved", certainty),
                    NOW,
                )
                .unwrap();
        }
        "ledger" => {
            ledger.record_event(TARGET, SUBSCRIPTION, &fact("cursor-0001", "new"), NOW).unwrap();
        }
        "member" => {
            repository.submit(&submission("new-member")).unwrap();
        }
        "remote" => {
            repository
                .record_snapshot_watermark(&child.identity, JobEventSequence::of(SECOND_SEQUENCE))
                .unwrap();
        }
        "rollback" => {
            rusqlite::Connection::open(path).unwrap().execute_batch("CREATE TRIGGER refuse_generation_loss BEFORE DELETE ON recovery_fact BEGIN SELECT RAISE(ABORT, 'injected recovery deletion failure'); END;").unwrap();
        }
        _ => {}
    }
}

/// Supplies the generation, revision and observation time for one refusal case.
fn generation_loss_arguments(defect: &str) -> (u64, u64, u64) {
    (
        if defect == "same-generation" {
            GENERATION
        } else if defect == "zero-generation" {
            0
        } else {
            GENERATION + 1
        },
        if defect == "revision" { 1 } else { HELD_RECOVERY_REVISION },
        if defect == "backward-time" {
            NOW - 1
        } else if defect == "overflow-time" {
            u64::MAX
        } else {
            NOW
        },
    )
}
