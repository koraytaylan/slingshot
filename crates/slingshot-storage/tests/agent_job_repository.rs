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
    PHYSICAL_JOBS_PER_SUBMISSION, SubmissionContracts, SubmissionIdentity, SubmissionOutcome,
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
    EventFact {
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
        .install_high_water(TARGET, SUBSCRIPTION, LATER_GENERATION, "cursor-0100", "contents-high")
        .expect("a reset heals it");
    let row = ledger.read_subscription(TARGET, SUBSCRIPTION);
    let row = row.expect("reads").expect("it is there");
    assert_eq!(row.cursor.as_deref(), Some("cursor-0100"));
    assert_eq!(row.high_water_cursor.as_deref(), Some("cursor-0100"));
    assert_eq!(row.agent_event_store_generation, LATER_GENERATION);
    assert_eq!(row.unresolved_incident, None);
    assert_eq!(row.unresolved_incident_count, 0);
    assert!(matches!(
        ledger.install_high_water(TARGET, "another-subscription", GENERATION, "c", "d"),
        Err(AgentRepositoryFailure::NoSuchSubscription { .. })
    ));
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
