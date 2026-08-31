//! What an event is allowed to conclude, and what it is not.
//!
//! The two folds are separate here because they are separate in the code, and
//! the cases that matter are the ones where they must disagree. An event about
//! work this daemon does not hold moves the stream and no job; an event whose
//! binding does not check out moves neither; and an event that arrives twice
//! with two different accounts moves nothing at all while asking a different
//! authority for help.
//!
//! The transition table is exercised exhaustively rather than by example,
//! because the interesting entry is the one that looks wrong: a physical
//! requeue arrives as another start, and reading that as a return to the queue
//! would make Sling's at-least-once delivery look like work that stopped.

use slingshot_agent_connection::job_event_reducer::{
    AssociationBinding, JobDisposition, ObservedJobEvent, ReducerRefusal, RetainedJob, reduce,
};
use slingshot_agent_connection::server_sent_event_decoder::TerminalCorrelation;
use slingshot_agent_connection::subscription_event_fold::{
    FoldRefusal, SubscriptionDisposition, SubscriptionFact, SubscriptionFold,
};
use slingshot_agent_protocol::identity::DocumentProvenance;
use slingshot_agent_protocol::job_contract::JobEventKind;
use slingshot_agent_protocol::wire_contract::ExpectedProvenance;
use slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract;
use slingshot_domain::command::schema::canonical_contract_digest;
use slingshot_domain::remote_job::{
    AgentJobState, EventStreamCursor, JobEventSequence, RemoteJobFailure, RemoteJobObservation,
};
use slingshot_domain::selected_command_contract_identity::SelectedCommandContractIdentity;

/// Where the vectors this suite is driven from live.
const FIXTURES: &str = "tests/fixtures/job-event-reducer.jsonl";

/// The subscription every vector is about.
const SUBSCRIPTION: &str = "daemon-subscription-one";

/// The generation every vector is about.
const GENERATION: u64 = 7;

/// A later generation, after the store has been rebuilt.
const LATER_GENERATION: u64 = 8;

/// The command these events are about.
const COMMAND: &str = "query_paths";

/// Another published command, to substitute one wire name for another.
const OTHER_COMMAND: &str = "create_page";

/// The environment revision this operation was submitted under.
const REVISION: &str = "environment-revision-one";

/// Another environment revision, which nothing here was submitted under.
const OTHER_REVISION: &str = "environment-revision-two";

/// The submission these events are about.
const SUBMITTED_DIGEST: &str = "1111111111111111111111111111111111111111111111111111111111111111";

/// The submission the same command with different arguments would produce.
const OTHER_ARGUMENTS_DIGEST: &str =
    "2222222222222222222222222222222222222222222222222222222222222222";

/// A digest substituted where a real one belongs.
const SUBSTITUTED_DIGEST: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// Another semantic contract version, which no installed command carries.
const OTHER_CONTRACT_VERSION: &str = "second";

/// The sequence one event after a job's first.
const SECOND_SEQUENCE: u64 = 2;

/// The sequence a retained running job has already applied.
const APPLIED_SEQUENCE: u64 = 3;

/// A sequence a snapshot has already accounted for.
const COVERED_SEQUENCE: u64 = 4;

/// The watermark that snapshot left.
const COVERED_WATERMARK: u64 = 5;

/// The sequence the snapshotted job sits at.
const SNAPSHOTTED_SEQUENCE: u64 = 6;

/// How many attempts a job that has been retried once has had.
const SECOND_ATTEMPT: u64 = 2;

/// How far along a job that has reported once says it is.
const SOME_PROGRESS: u64 = 40;

/// How far along a job that has reported twice says it is.
const MORE_PROGRESS: u64 = 80;

/// Returns every vector the fixture holds.
fn vectors() -> Vec<serde_json::Value> {
    let text = std::fs::read_to_string(FIXTURES).expect("the vectors are readable");
    text.lines().map(|line| serde_json::from_str(line).expect("each line is one vector")).collect()
}

/// Returns every vector of `kind`.
fn vectors_of(kind: &str) -> Vec<serde_json::Value> {
    vectors().into_iter().filter(|vector| vector["kind"].as_str() == Some(kind)).collect()
}

/// Returns what this build has, for the command these events are about.
fn installed_provenance() -> ExpectedProvenance {
    ExpectedProvenance {
        canonical_json_contract_digest: canonical_contract_digest(),
        command_contract: SelectedCommandContractIdentity::installed(COMMAND)
            .expect("the command is published"),
        transport_contract_digest: AuthorAgentTransportContract::embedded_digest(),
    }
}

/// Returns what must agree before an associated job moves.
fn binding() -> AssociationBinding {
    AssociationBinding {
        expected_provenance: installed_provenance(),
        selected_environment_revision: REVISION.to_owned(),
        submitted_command_digest: SUBMITTED_DIGEST.to_owned(),
    }
}

/// Returns the correlation a truthful ending carries.
fn correlation() -> TerminalCorrelation {
    TerminalCorrelation {
        provenance: installed_provenance().provenance(),
        submitted_command_digest: SUBMITTED_DIGEST.to_owned(),
    }
}

/// Returns the kind `spelling` names.
fn kind_named(spelling: &str) -> JobEventKind {
    match spelling {
        "accepted" => JobEventKind::Accepted,
        "started" => JobEventKind::Started,
        "progress" => JobEventKind::Progress,
        "succeeded" => JobEventKind::Succeeded,
        "failed" => JobEventKind::Failed,
        other => panic!("{other} is a kind this suite does not name"),
    }
}

/// Returns the state `spelling` names.
fn state_named(spelling: &str) -> AgentJobState {
    match spelling {
        "queued" => AgentJobState::Queued,
        "running" => AgentJobState::Running,
        "succeeded" => AgentJobState::Succeeded,
        "failed" => AgentJobState::Failed,
        other => panic!("{other} is a state this suite does not name"),
    }
}

/// Returns the disposition `spelling` names.
fn disposition_named(spelling: &str) -> JobDisposition {
    match spelling {
        "applied" => JobDisposition::Applied,
        "exact-replay" => JobDisposition::ExactReplay,
        "stale-cursor-only" => JobDisposition::StaleCursorOnly,
        "needs-snapshot" => JobDisposition::NeedsSnapshot,
        "integrity-conflict" => JobDisposition::IntegrityConflictNeedsReconciliation,
        other => panic!("{other} is a disposition this suite does not name"),
    }
}

/// Returns one event of `kind` at `sequence`.
fn event_at(kind: JobEventKind, sequence: u64, attempt: u64, progress: u64) -> ObservedJobEvent {
    ObservedJobEvent {
        attempt,
        correlation: kind.is_terminal().then(correlation),
        kind,
        progress,
        selected_environment_revision: REVISION.to_owned(),
        sequence: JobEventSequence::of(sequence),
    }
}

/// Returns what is held about a job in `state` at `applied`.
fn retained(state: AgentJobState, applied: u64, watermark: u64) -> RetainedJob {
    RetainedJob {
        observation: RemoteJobObservation {
            applied_sequence: JobEventSequence::of(applied),
            attempt: 0,
            progress: 0,
            state,
        },
        snapshot_watermark: JobEventSequence::of(watermark),
    }
}

#[test]
fn the_domain_transition_table_decides_every_event_kind() {
    for vector in vectors_of("transition") {
        let name = vector["name"].as_str().expect("a name");
        let held = state_named(vector["held_state"].as_str().expect("a state"));
        let kind = kind_named(vector["event_kind"].as_str().expect("a kind"));
        let applied = 1;
        let event = event_at(kind, applied + 1, 0, 0);
        let produced = reduce(Some(&retained(held, applied, applied)), &binding(), &event);
        if vector["allowed"].as_bool().expect("an expectation") {
            let (disposition, advanced) =
                produced.unwrap_or_else(|refusal| panic!("{name}: {refusal}"));
            assert_eq!(disposition, JobDisposition::Applied, "{name}");
            assert_eq!(
                advanced.expect("an applied event leaves a state").state,
                state_named(vector["becomes"].as_str().expect("a state")),
                "{name}"
            );
        } else {
            let refusal = produced.expect_err(&format!("{name} is refused"));
            assert!(
                matches!(refusal, ReducerRefusal::Job(_)),
                "{name}: the domain owns the table, and the reducer only reads it"
            );
        }
    }
}

#[test]
fn a_physical_requeue_is_the_same_work_running_with_monotonic_metadata() {
    let held = retained(AgentJobState::Running, 1, 1);
    let requeued = event_at(JobEventKind::Started, SECOND_SEQUENCE, SECOND_ATTEMPT, SOME_PROGRESS);
    let (disposition, advanced) =
        reduce(Some(&held), &binding(), &requeued).expect("a requeue carries the same work");
    assert_eq!(disposition, JobDisposition::Applied);
    let advanced = advanced.expect("an applied event leaves a state");
    assert_eq!(advanced.state, AgentJobState::Running);
    assert_eq!(advanced.attempt, SECOND_ATTEMPT);
    assert_eq!(advanced.progress, SOME_PROGRESS);

    let reported =
        RetainedJob { observation: advanced, snapshot_watermark: JobEventSequence::first() };
    let regressed = event_at(JobEventKind::Progress, APPLIED_SEQUENCE, SECOND_ATTEMPT, 0);
    assert!(matches!(
        reduce(Some(&reported), &binding(), &regressed),
        Err(ReducerRefusal::Job(RemoteJobFailure::ProgressRegressed { .. }))
    ));
    let regressed = event_at(JobEventKind::Progress, APPLIED_SEQUENCE, 1, MORE_PROGRESS);
    assert!(matches!(
        reduce(Some(&reported), &binding(), &regressed),
        Err(ReducerRefusal::Job(RemoteJobFailure::AttemptRegressed { .. }))
    ));
}

#[test]
fn sequences_decide_between_applying_asking_and_ignoring() {
    for vector in vectors_of("sequence") {
        let name = vector["name"].as_str().expect("a name");
        let held = retained(
            AgentJobState::Running,
            vector["applied"].as_u64().expect("a sequence"),
            vector["watermark"].as_u64().expect("a watermark"),
        );
        let event = event_at(
            JobEventKind::Progress,
            vector["sequence"].as_u64().expect("a sequence"),
            0,
            0,
        );
        let (disposition, advanced) =
            reduce(Some(&held), &binding(), &event).expect("these are all believable");
        assert_eq!(
            disposition,
            disposition_named(vector["disposition"].as_str().expect("a disposition")),
            "{name}"
        );
        assert_eq!(
            advanced.is_some(),
            disposition.changed_state(),
            "{name}: only an applied event leaves a state"
        );
        if matches!(disposition, JobDisposition::NeedsSnapshot) {
            assert!(
                disposition.needs_authority(),
                "{name}: filling a gap from the event after it invents the events inside it"
            );
        }
    }
}

#[test]
fn an_exact_repeat_is_a_replay_and_a_differing_repeat_is_a_conflict() {
    let held = RetainedJob {
        observation: RemoteJobObservation {
            applied_sequence: JobEventSequence::of(APPLIED_SEQUENCE),
            attempt: SECOND_ATTEMPT,
            progress: SOME_PROGRESS,
            state: AgentJobState::Running,
        },
        snapshot_watermark: JobEventSequence::first(),
    };
    let same = event_at(JobEventKind::Progress, APPLIED_SEQUENCE, SECOND_ATTEMPT, SOME_PROGRESS);
    assert_eq!(
        reduce(Some(&held), &binding(), &same).expect("a replay is ordinary"),
        (JobDisposition::ExactReplay, None)
    );
    let differing =
        event_at(JobEventKind::Progress, APPLIED_SEQUENCE, SECOND_ATTEMPT, MORE_PROGRESS);
    let (disposition, advanced) =
        reduce(Some(&held), &binding(), &differing).expect("a conflict is an answer, not an error");
    assert_eq!(disposition, JobDisposition::IntegrityConflictNeedsReconciliation);
    assert_eq!(advanced, None, "a job whose accounts disagree is a job whose state nobody knows");
    assert!(disposition.needs_authority() && !disposition.changed_state());
}

#[test]
fn a_lower_sequence_a_snapshot_already_covers_needs_no_digest_to_agree_with() {
    let held = retained(AgentJobState::Running, SNAPSHOTTED_SEQUENCE, COVERED_WATERMARK);
    let covered = event_at(JobEventKind::Progress, COVERED_SEQUENCE, 0, 0);
    assert_eq!(
        reduce(Some(&held), &binding(), &covered).expect("old news is believable"),
        (JobDisposition::StaleCursorOnly, None),
        "everything at or below the watermark was settled by authority rather than by events"
    );
    assert!(held.snapshot_watermark.follows(covered.sequence));
}

#[test]
fn an_event_for_work_this_daemon_does_not_hold_moves_no_job() {
    for kind in [JobEventKind::Accepted, JobEventKind::Progress, JobEventKind::Succeeded] {
        let event = event_at(kind, 1, 0, 0);
        assert_eq!(
            reduce(None, &binding(), &event).expect("an unheld job is not a refusal"),
            (JobDisposition::StaleCursorOnly, None),
            "inventing a local row from a stream event would hold work nobody submitted"
        );
    }
    let fold = SubscriptionFold::opened(SUBSCRIPTION, GENERATION);
    let (disposition, advanced) =
        fold.folded(&fact("cursor-0001", "contents-one")).expect("the stream still moved");
    assert_eq!(disposition, SubscriptionDisposition::Advanced);
    assert_eq!(advanced.cursor().map(EventStreamCursor::as_text), Some("cursor-0001"));
}

/// Returns one subscription fact at `cursor`.
fn fact(cursor: &str, canonical_digest: &str) -> SubscriptionFact {
    SubscriptionFact {
        agent_event_store_generation: GENERATION,
        canonical_digest: canonical_digest.to_owned(),
        cursor: EventStreamCursor::new(cursor).expect("these cursors are short"),
        daemon_subscription_identifier: SUBSCRIPTION.to_owned(),
    }
}

#[test]
fn every_substituted_binding_field_advances_neither_fold() {
    let held = retained(AgentJobState::Running, 1, 1);
    for vector in vectors_of("substitution") {
        let name = vector["field"].as_str().expect("a field");
        let (binding, event) = substituted(name);
        let refusal = reduce(Some(&held), &binding, &event)
            .expect_err(&format!("{name} names another submission"));
        assert!(
            !matches!(refusal, ReducerRefusal::Job(_)),
            "{name}: the binding is checked before the transition table is consulted"
        );
    }
}

/// Returns the binding and event one named substitution produces.
fn substituted(field: &str) -> (AssociationBinding, ObservedJobEvent) {
    let mut binding = binding();
    let mut event = event_at(JobEventKind::Succeeded, SECOND_SEQUENCE, 0, 0);
    let mut provenance = installed_provenance().provenance();
    if !substituted_beside_the_provenance(field, &mut binding, &mut event) {
        substituted_within_the_provenance(field, &mut provenance);
    }
    event.correlation = Some(TerminalCorrelation {
        provenance,
        submitted_command_digest: SUBMITTED_DIGEST.to_owned(),
    });
    (binding, event)
}

/// Substitutes one field the provenance does not carry, and says whether it did.
fn substituted_beside_the_provenance(
    field: &str,
    binding: &mut AssociationBinding,
    event: &mut ObservedJobEvent,
) -> bool {
    match field {
        "selected-environment-revision" => {
            event.selected_environment_revision = OTHER_REVISION.to_owned();
        }
        "submitted-digest" => binding.submitted_command_digest = SUBSTITUTED_DIGEST.to_owned(),
        "same-command-different-arguments" => {
            binding.submitted_command_digest = OTHER_ARGUMENTS_DIGEST.to_owned();
        }
        _ => return false,
    }
    true
}

/// Substitutes one field of the provenance an ending carries.
fn substituted_within_the_provenance(field: &str, provenance: &mut DocumentProvenance) {
    match field {
        "transport-contract" => {
            provenance.transport_contract_digest = SUBSTITUTED_DIGEST.to_owned()
        }
        "canonical-byte-contract" => {
            provenance.canonical_json_contract_digest = SUBSTITUTED_DIGEST.to_owned();
        }
        "argument-schema" => {
            provenance.command_contract.argument_schema_digest = SUBSTITUTED_DIGEST.to_owned();
        }
        "result-schema" => {
            provenance.command_contract.result_schema_digest = SUBSTITUTED_DIGEST.to_owned();
        }
        "contract-limits" => {
            provenance.command_contract.command_contract_limits_digest =
                SUBSTITUTED_DIGEST.to_owned();
        }
        "semantic-version" => {
            provenance.command_contract.command_semantic_contract_version =
                OTHER_CONTRACT_VERSION.to_owned();
        }
        "wire-name" => provenance.command_contract.command_wire_name = OTHER_COMMAND.to_owned(),
        other => panic!("{other} is a substitution this suite does not stage"),
    }
}

#[test]
fn a_valid_shape_from_the_same_command_with_other_arguments_is_still_another_submission() {
    let held = retained(AgentJobState::Running, 1, 1);
    let mut binding = binding();
    binding.submitted_command_digest = OTHER_ARGUMENTS_DIGEST.to_owned();
    let event = event_at(JobEventKind::Succeeded, SECOND_SEQUENCE, 0, 0);
    assert_eq!(
        reduce(Some(&held), &binding, &event),
        Err(ReducerRefusal::AnotherSubmission),
        "the contracts agree and the digest does not, which is exactly what a shape check misses"
    );
}

#[test]
fn an_ending_carries_a_correlation_and_nothing_else_does() {
    let held = retained(AgentJobState::Running, 1, 1);
    let mut bare_ending = event_at(JobEventKind::Succeeded, SECOND_SEQUENCE, 0, 0);
    bare_ending.correlation = None;
    assert_eq!(
        reduce(Some(&held), &binding(), &bare_ending),
        Err(ReducerRefusal::CorrelationMisplaced)
    );
    let mut correlated_progress = event_at(JobEventKind::Progress, SECOND_SEQUENCE, 0, 0);
    correlated_progress.correlation = Some(correlation());
    assert_eq!(
        reduce(Some(&held), &binding(), &correlated_progress),
        Err(ReducerRefusal::CorrelationMisplaced)
    );
}

#[test]
fn the_subscription_fold_answers_only_about_positions() {
    for vector in vectors_of("subscription") {
        let name = vector["name"].as_str().expect("a name");
        let mut held = SubscriptionFold::opened(SUBSCRIPTION, GENERATION);
        if let Some(cursor) = vector["held_cursor"].as_str() {
            held = held
                .folded(&fact(cursor, vector["held_digest"].as_str().expect("a digest")))
                .expect("the held position is this subscription's")
                .1;
        }
        let (disposition, next) = held
            .folded(&fact(
                vector["cursor"].as_str().expect("a cursor"),
                vector["canonical_digest"].as_str().expect("a digest"),
            ))
            .expect("these are all this subscription's");
        let spelling = match disposition {
            SubscriptionDisposition::Advanced => "advanced",
            SubscriptionDisposition::ExactReplay => "exact-replay",
            SubscriptionDisposition::StaleCursorOnly => "stale-cursor-only",
            SubscriptionDisposition::IntegrityConflictNeedsReconciliation => "integrity-conflict",
        };
        assert_eq!(spelling, vector["disposition"].as_str().expect("a disposition"), "{name}");
        if matches!(disposition, SubscriptionDisposition::Advanced) {
            assert_eq!(
                next.cursor().map(EventStreamCursor::as_text),
                vector["cursor"].as_str(),
                "{name}: advancing means sitting where the event was"
            );
        } else {
            assert_eq!(next, held, "{name}: only advancing moves the fold");
        }
    }
}

#[test]
fn one_position_with_two_accounts_stops_the_subscription_rather_than_the_job() {
    let fold = SubscriptionFold::opened(SUBSCRIPTION, GENERATION)
        .folded(&fact("cursor-0005", "contents-five"))
        .expect("the first position is ours")
        .1;
    let (disposition, unchanged) = fold
        .folded(&fact("cursor-0005", "contents-other"))
        .expect("a conflict is an answer, not a refusal");
    assert_eq!(disposition, SubscriptionDisposition::IntegrityConflictNeedsReconciliation);
    assert!(!disposition.permits_streaming());
    assert_eq!(unchanged, fold, "the fold keeps what it had, because it cannot choose");
    assert_eq!(unchanged.canonical_digest(), Some("contents-five"));
}

#[test]
fn an_event_from_another_subscription_or_generation_advances_neither_fold() {
    let fold = SubscriptionFold::opened(SUBSCRIPTION, GENERATION);
    let mut elsewhere = fact("cursor-0001", "contents-one");
    elsewhere.daemon_subscription_identifier = "another-subscription".to_owned();
    assert_eq!(fold.folded(&elsewhere), Err(FoldRefusal::AnotherSubscription));
    let mut rebuilt = fact("cursor-0001", "contents-one");
    rebuilt.agent_event_store_generation = LATER_GENERATION;
    assert_eq!(
        fold.folded(&rebuilt),
        Err(FoldRefusal::AnotherGeneration { held: GENERATION, named: LATER_GENERATION })
    );
    assert_eq!(fold.cursor(), None, "a refused event leaves the fold exactly as it was");
    assert_eq!(fold.generation(), GENERATION);
}

#[test]
fn an_ending_is_never_left_and_no_conflict_invents_one() {
    for ending in [AgentJobState::Succeeded, AgentJobState::Failed] {
        let held = retained(ending, 1, 1);
        for kind in [JobEventKind::Accepted, JobEventKind::Started, JobEventKind::Progress] {
            assert!(matches!(
                reduce(Some(&held), &binding(), &event_at(kind, SECOND_SEQUENCE, 0, 0)),
                Err(ReducerRefusal::Job(RemoteJobFailure::EndingIsFinal { .. }))
            ));
        }
    }
    let running = RetainedJob {
        observation: RemoteJobObservation {
            applied_sequence: JobEventSequence::of(APPLIED_SEQUENCE),
            attempt: SECOND_ATTEMPT,
            progress: SOME_PROGRESS,
            state: AgentJobState::Running,
        },
        snapshot_watermark: JobEventSequence::first(),
    };
    let (disposition, advanced) = reduce(
        Some(&running),
        &binding(),
        &event_at(JobEventKind::Progress, APPLIED_SEQUENCE, SECOND_ATTEMPT, MORE_PROGRESS),
    )
    .expect("a conflict is an answer");
    assert_eq!(disposition, JobDisposition::IntegrityConflictNeedsReconciliation);
    assert_eq!(advanced, None, "no conflict promotes itself into an ending");
}
