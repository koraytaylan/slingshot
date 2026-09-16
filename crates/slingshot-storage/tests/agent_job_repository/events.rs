//! Durable events integration checks.

use super::*;
use slingshot_storage::agent_subscription_ledger::ActiveResetSnapshot;

/// Progress carried by the active-event fixture.
const EVENT_PROGRESS: u64 = 5;
/// Measured size supplied for the fixture event.
const EVENT_BYTES: u64 = 256;
/// Deliberately noncontiguous event sequence.
const GAP_SEQUENCE: u64 = 100;
/// Members the atomic reset must update together.
const RESET_MEMBERS: u64 = 2;
/// First sequence supplied by the reset snapshots.
const RESET_FIRST_SEQUENCE: u64 = 3;
/// First progress value supplied by the reset snapshots.
const RESET_FIRST_PROGRESS: u64 = 10;
/// Elapsed time deducted from retention and added to the reset clock.
const RESET_ELAPSED_MILLISECONDS: u64 = 100;

#[test]
fn active_event_commits_cursor_job_and_physical_identity_or_rolls_back_all_three() {
    for defect in [
        "",
        "known-physical",
        "job-write",
        "physical-write",
        "event-write",
        "ledger-moved",
        "physical-moved",
        "remote-moved",
        "missing-owner",
        "wrong-owner",
        "gap",
        "terminal",
        "generation",
        "wrong-job",
        "wrong-sequence",
        "bad-digest",
        "bad-physical",
        "unsafe-cursor",
        "overflow",
        "backwards-time",
        "capacity",
        "incident",
        "known-success",
        "local-moved",
    ] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("atomic-event.sqlite3");
        let ledger =
            AgentSubscriptionLedger::new(OperationDatabase::open(&path, settings()).unwrap());
        let repository =
            AgentJobRepository::new(OperationDatabase::open(&path, settings()).unwrap());
        ledger.open_subscription(TARGET, SUBSCRIPTION, GENERATION, NOW).unwrap();
        let child = submission("active-event");
        repository.submit(&child).unwrap();
        if defect != "missing-owner" {
            admit_reset_owner(&path, &child);
        }
        let record_success = || {
            use slingshot_domain::operation::{
                OperationFact, RecoveryCategory, RecoveryExecutionEvidence, RecoveryFact,
            };
            use slingshot_storage::operation_repository::OperationRepository;
            OperationRepository::new(OperationDatabase::open(&path, settings()).unwrap())
                .apply(
                    TARGET,
                    &child.identity.operation_identifier,
                    1,
                    &OperationFact::Recovery {
                        recovery: RecoveryFact {
                            attempt_count: 0,
                            category: RecoveryCategory::ResultAcquisition,
                            detail: "pending".into(),
                            evidence: RecoveryExecutionEvidence::AuthoritativeRemoteSuccess,
                            manual_resume_eligible: false,
                            retry_delay_milliseconds: 0,
                            retry_observed_at_unix_milliseconds: NOW,
                        },
                    },
                    NOW,
                )
                .unwrap();
        };
        if defect == "known-success" {
            record_success();
        }
        prepare_active_event_history(defect, &ledger, &repository, &child);
        let view = ledger.read_recovery_view(TARGET, SUBSCRIPTION).unwrap();
        let mut observation = RemoteJobObservation {
            applied_sequence: JobEventSequence::of(child.observation.applied_sequence.value() + 1),
            attempt: 1,
            progress: EVENT_PROGRESS,
            state: AgentJobState::Running,
        };
        let mut event = EventFact {
            agent_event_store_generation: GENERATION,
            agent_operation_identifier: Some(child.identity.agent_operation_identifier.clone()),
            canonical_digest: "a".repeat(DIGEST_CHARACTERS),
            cursor: "cursor-0002".into(),
            event_bytes: EVENT_BYTES,
            job_sequence: Some(observation.applied_sequence.value()),
        };
        inject_active_event_storage_defect(defect, &path, &ledger, &repository, &child);
        alter_active_event_evidence(defect, &mut observation, &mut event);
        if defect == "local-moved" {
            record_success();
        }
        let before = ledger.read_recovery_view(TARGET, SUBSCRIPTION).unwrap();
        let physical_before =
            repository.physical_jobs(TARGET, &child.identity.agent_operation_identifier).unwrap();
        let other =
            AgentSubscriptionLedger::new(OperationDatabase::open(&path, settings()).unwrap());
        let writer = if defect == "wrong-owner" { &other } else { &ledger };
        let result = writer.record_active_event(
            &view,
            &event,
            observation,
            if defect == "bad-physical" { "" } else { "physical-event" },
            if defect == "backwards-time" { NOW - 1 } else { NOW + 1 },
        );
        let success = ["", "known-physical"].contains(&defect);
        assert_eq!(result.is_ok(), success, "{defect}: {result:?}");
        let reopened =
            AgentSubscriptionLedger::new(OperationDatabase::open(&path, settings()).unwrap());
        let after = reopened.read_recovery_view(TARGET, SUBSCRIPTION).unwrap();
        let physical_after =
            repository.physical_jobs(TARGET, &child.identity.agent_operation_identifier).unwrap();
        if success {
            assert_eq!(result.unwrap(), LedgerOutcome::Advanced);
            assert_eq!(after.ledger().cursor.as_deref(), Some("cursor-0002"));
            assert_eq!(after.ledger().event_rows, 1);
            assert_eq!(after.ledger().event_bytes, EVENT_BYTES);
            let mut expected_child = child.clone();
            expected_child.observation = observation;
            assert_eq!(
                after.members(),
                &[expected_child],
                "retention and snapshot facts must not change"
            );
            assert_eq!(physical_after, ["physical-event"]);
            assert!(
                ledger
                    .record_active_event(&view, &event, observation, "physical-event", NOW + 1)
                    .is_err()
            );
        } else {
            assert_eq!(after.ledger(), before.ledger(), "{defect}: cursor/accounting rolled back");
            assert_eq!(after.members(), before.members(), "{defect}: job rolled back");
            assert_eq!(
                physical_after, physical_before,
                "{defect}: physical association rolled back"
            );
        }
    }
}

/// Installs retained history before capturing the view under test.
fn prepare_active_event_history(
    defect: &str,
    ledger: &AgentSubscriptionLedger,
    repository: &AgentJobRepository,
    child: &AgentSubmission,
) {
    if defect == "known-physical" {
        repository.record_physical_job(&child.identity, "physical-event", NOW).unwrap();
    }
    if defect == "capacity" {
        for index in 0..PHYSICAL_JOBS_PER_SUBMISSION {
            repository
                .record_physical_job(&child.identity, &format!("physical-{index}"), NOW)
                .unwrap();
        }
    }
    if defect == "incident" {
        ledger.record_event(TARGET, SUBSCRIPTION, &fact("cursor-0001", "first"), NOW).unwrap();
        ledger.record_event(TARGET, SUBSCRIPTION, &fact("cursor-0001", "other"), NOW).unwrap();
    }
}

/// Changes persisted state or injects a write fault after capturing the view.
fn inject_active_event_storage_defect(
    defect: &str,
    path: &std::path::Path,
    ledger: &AgentSubscriptionLedger,
    repository: &AgentJobRepository,
    child: &AgentSubmission,
) {
    match defect {
        "job-write" => {
            rusqlite::Connection::open(path).unwrap().execute_batch("CREATE TRIGGER refuse_event_job BEFORE UPDATE OF applied_sequence ON agent_operation BEGIN SELECT RAISE(ABORT, 'injected job failure'); END;").unwrap();
        }
        "physical-write" => {
            rusqlite::Connection::open(path).unwrap().execute_batch("CREATE TRIGGER refuse_event_physical BEFORE INSERT ON agent_physical_job BEGIN SELECT RAISE(ABORT, 'injected physical failure'); END;").unwrap();
        }
        "event-write" => {
            rusqlite::Connection::open(path).unwrap().execute_batch("CREATE TRIGGER refuse_event_row BEFORE INSERT ON subscription_event BEGIN SELECT RAISE(ABORT, 'injected event failure'); END;").unwrap();
        }
        "ledger-moved" => {
            ledger.record_event(TARGET, SUBSCRIPTION, &fact("cursor-0001", "old"), NOW).unwrap();
        }
        "physical-moved" => {
            repository.record_physical_job(&child.identity, "concurrent", NOW).unwrap();
        }
        "remote-moved" => {
            repository.record_snapshot_watermark(&child.identity, JobEventSequence::of(1)).unwrap();
        }
        _ => {}
    }
}

/// Changes only the offered event or observation, leaving the database intact.
fn alter_active_event_evidence(
    defect: &str,
    observation: &mut RemoteJobObservation,
    event: &mut EventFact,
) {
    match defect {
        "gap" => {
            observation.applied_sequence = JobEventSequence::of(GAP_SEQUENCE);
            event.job_sequence = Some(GAP_SEQUENCE);
        }
        "terminal" => observation.state = AgentJobState::Succeeded,
        "generation" => event.agent_event_store_generation += 1,
        "wrong-job" => event.agent_operation_identifier = Some("other".into()),
        "wrong-sequence" => event.job_sequence = None,
        "bad-digest" => event.canonical_digest = "A".repeat(DIGEST_CHARACTERS),
        "unsafe-cursor" => event.cursor = "unsafe\r\n".into(),
        "overflow" => observation.attempt = u64::MAX,
        _ => {}
    }
}

#[test]
fn active_reset_commits_every_member_and_boundary_or_rolls_back_every_write() {
    for defect in [
        "",
        "older",
        "missing",
        "duplicate",
        "new-member",
        "remote-moved",
        "physical-moved",
        "local-moved",
        "delete",
        "second-write",
        "generation",
        "terminal",
        "zero-retention",
        "omitted-physical",
        "prior-generation",
        "missing-owner",
        "unsafe-watermark",
        "overflow",
        "stale-sequence",
    ] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("active-reset.sqlite3");
        let ledger =
            AgentSubscriptionLedger::new(OperationDatabase::open(&path, settings()).unwrap());
        let repository =
            AgentJobRepository::new(OperationDatabase::open(&path, settings()).unwrap());
        ledger.open_subscription(TARGET, SUBSCRIPTION, GENERATION, NOW).unwrap();
        ledger.record_event(TARGET, SUBSCRIPTION, &fact("cursor-0005", "old"), NOW).unwrap();
        ledger.record_event(TARGET, SUBSCRIPTION, &fact("cursor-0005", "conflict"), NOW).unwrap();
        let mut snapshots = prepare_active_reset_members(defect, &path, &repository);
        let view = ledger.read_recovery_view(TARGET, SUBSCRIPTION).unwrap();
        inject_active_reset_storage_defect(defect, &path, &repository, &view);
        alter_active_reset_evidence(defect, &mut snapshots);
        let before = ledger.read_recovery_view(TARGET, SUBSCRIPTION).unwrap();
        let before_physical: Vec<_> = before
            .members()
            .iter()
            .map(|child| {
                repository
                    .physical_jobs(TARGET, &child.identity.agent_operation_identifier)
                    .unwrap()
            })
            .collect();
        let result = ledger.install_active_snapshot_reset(
            &view,
            if defect == "generation" { LATER_GENERATION } else { GENERATION },
            "cursor-0100",
            &snapshots,
            NOW + RESET_ELAPSED_MILLISECONDS,
        );
        assert_eq!(result.is_ok(), defect.is_empty(), "{defect}: {result:?}");
        let reopened =
            AgentSubscriptionLedger::new(OperationDatabase::open(&path, settings()).unwrap());
        let after = reopened.read_recovery_view(TARGET, SUBSCRIPTION).unwrap();
        if defect.is_empty() {
            assert_eq!(after.ledger().cursor.as_deref(), Some("cursor-0100"));
            assert_eq!(after.ledger().canonical_digest, None);
            assert_eq!(after.ledger().unresolved_incident, None);
            assert_eq!(after.ledger().event_rows, 0);
            for (child, snapshot) in after.members().iter().zip(&snapshots) {
                assert_eq!(child.observation, snapshot.observation);
                assert_eq!(child.snapshot_watermark, snapshot.observation.applied_sequence);
                assert_eq!(
                    child.remaining_retention_milliseconds,
                    RETENTION - RESET_ELAPSED_MILLISECONDS
                );
                assert_eq!(
                    repository
                        .physical_jobs(TARGET, &child.identity.agent_operation_identifier)
                        .unwrap(),
                    snapshot.physical_sling_job_identifiers
                );
            }
        } else {
            assert_eq!(after.members(), before.members(), "{defect}");
            assert_eq!(after.ledger(), before.ledger(), "{defect}");
            for (child, physical) in after.members().iter().zip(&before_physical) {
                assert_eq!(
                    &repository
                        .physical_jobs(TARGET, &child.identity.agent_operation_identifier)
                        .unwrap(),
                    physical,
                    "{defect}"
                );
            }
        }
    }
}

#[test]
fn empty_recovery_rechecks_owner_membership_and_rolls_back_cursor_on_delete_failure() {
    for defect in ["owner", "new-member", "delete", "moved", "backward", "unsafe"] {
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
        if defect == "new-member" {
            AgentJobRepository::new(OperationDatabase::open(&path, settings()).unwrap())
                .submit(&submission("new"))
                .unwrap();
        }
        if defect == "delete" {
            rusqlite::Connection::open(&path).unwrap().execute_batch("CREATE TRIGGER reject_reset_delete BEFORE DELETE ON subscription_event BEGIN SELECT RAISE(ABORT, 'injected reset deletion failure'); END;").unwrap();
        }
        if defect == "moved" {
            ledger.record_event(TARGET, SUBSCRIPTION, &fact("cursor-0006", "six"), NOW).unwrap();
        }
        let before = ledger.read_subscription(TARGET, SUBSCRIPTION).unwrap().unwrap();
        let another =
            AgentSubscriptionLedger::new(OperationDatabase::open(&path, settings()).unwrap());
        let receiver = if defect == "owner" { &another } else { &ledger };
        let cursor = match defect {
            "backward" => "cursor-0001",
            "unsafe" => "bad\r\ncursor",
            _ => "cursor-0100",
        };
        assert!(receiver.install_empty_recovery(&view, GENERATION, cursor).is_err(), "{defect}");
        assert_eq!(
            ledger.read_subscription(TARGET, SUBSCRIPTION).unwrap().unwrap(),
            before,
            "{defect}"
        );
        assert_eq!(
            another.read_subscription(TARGET, SUBSCRIPTION).unwrap().unwrap(),
            before,
            "{defect}"
        );
        // A failed post-update delete must leave the retained event and counters intact.
        if defect == "delete" {
            assert_eq!(
                ledger
                    .record_event(TARGET, SUBSCRIPTION, &fact("cursor-0005", "contents-five"), NOW)
                    .unwrap(),
                LedgerOutcome::ExactReplay
            );
        }
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

/// Builds both retained members before the reset view is captured.
fn prepare_active_reset_members(
    defect: &str,
    path: &std::path::Path,
    repository: &AgentJobRepository,
) -> Vec<ActiveResetSnapshot> {
    let mut snapshots = Vec::new();
    for number in 0..RESET_MEMBERS {
        let mut child = submission(&format!("member-{number}"));
        if number == 1 && defect == "prior-generation" {
            child.identity.agent_event_store_generation -= 1;
        }
        repository.submit(&child).unwrap();
        if number != 1 || defect != "missing-owner" {
            admit_reset_owner(path, &child);
        }
        repository
            .record_physical_job(&child.identity, &format!("job-{number}-existing"), NOW)
            .unwrap();
        snapshots.push(ActiveResetSnapshot {
            agent_operation_identifier: child.identity.agent_operation_identifier,
            observation: running(RESET_FIRST_SEQUENCE + number, 1, RESET_FIRST_PROGRESS + number),
            physical_sling_job_identifiers: vec![
                format!("job-{number}-existing"),
                format!("job-{number}-new"),
            ],
            remaining_retention_milliseconds: RETENTION - RESET_ELAPSED_MILLISECONDS,
            subscription_watermark: "cursor-0100".into(),
        });
    }
    snapshots
}

/// Alters persisted recovery evidence or injects a transactional reset fault.
fn inject_active_reset_storage_defect(
    defect: &str,
    path: &std::path::Path,
    repository: &AgentJobRepository,
    view: &slingshot_storage::agent_subscription_ledger::SubscriptionRecoveryView<'_>,
) {
    match defect {
        "new-member" => {
            repository.submit(&submission("member-2")).unwrap();
        }
        "remote-moved" => {
            repository
                .record_snapshot_watermark(
                    &view.members()[1].identity,
                    JobEventSequence::of(SECOND_SEQUENCE),
                )
                .unwrap();
        }
        "physical-moved" => {
            repository
                .record_physical_job(&view.members()[1].identity, "concurrent-job", NOW)
                .unwrap();
        }
        "local-moved" => {
            use slingshot_domain::operation::{
                OperationExecutionCertainty, OperationFact, RecoveryCategory,
                RecoveryExecutionEvidence, RecoveryFact,
            };
            use slingshot_storage::operation_repository::OperationRepository;
            let child = &view.members()[1];
            OperationRepository::new(OperationDatabase::open(path, settings()).unwrap())
                .apply(
                    TARGET,
                    &child.identity.operation_identifier,
                    1,
                    &OperationFact::Recovery {
                        recovery: RecoveryFact {
                            attempt_count: 0,
                            category: RecoveryCategory::OperationLookup,
                            detail: "pending".into(),
                            evidence: RecoveryExecutionEvidence::ExecutionCertainty {
                                certainty: OperationExecutionCertainty::RemoteOutcomeUnknown,
                            },
                            manual_resume_eligible: false,
                            retry_delay_milliseconds: 0,
                            retry_observed_at_unix_milliseconds: NOW,
                        },
                    },
                    NOW,
                )
                .unwrap();
        }
        "delete" => {
            rusqlite::Connection::open(path).unwrap().execute_batch("CREATE TRIGGER refuse_reset_delete BEFORE DELETE ON subscription_event BEGIN SELECT RAISE(ABORT, 'injected deletion failure'); END;").unwrap();
        }
        "second-write" => {
            rusqlite::Connection::open(path).unwrap().execute_batch("CREATE TRIGGER refuse_second_watermark BEFORE UPDATE OF snapshot_watermark ON agent_operation WHEN OLD.agent_operation_identifier = 'agent-operation-member-1' BEGIN SELECT RAISE(ABORT, 'injected second snapshot failure'); END;").unwrap();
        }
        _ => {}
    }
}

/// Alters the supplied snapshots without changing the retained database view.
fn alter_active_reset_evidence(defect: &str, snapshots: &mut Vec<ActiveResetSnapshot>) {
    match defect {
        "older" => snapshots[1].subscription_watermark = "cursor-0099".into(),
        "missing" => {
            snapshots.pop();
        }
        "duplicate" => snapshots[1] = snapshots[0].clone(),
        "terminal" => snapshots[1].observation.state = AgentJobState::Succeeded,
        "zero-retention" => snapshots[1].remaining_retention_milliseconds = 0,
        "omitted-physical" => {
            snapshots[1].physical_sling_job_identifiers.remove(0);
        }
        "unsafe-watermark" => snapshots[1].subscription_watermark = "z\r\nunsafe".into(),
        "overflow" => snapshots[1].observation.applied_sequence = JobEventSequence::of(u64::MAX),
        "stale-sequence" => snapshots[1].observation.applied_sequence = JobEventSequence::of(0),
        _ => {}
    }
}
