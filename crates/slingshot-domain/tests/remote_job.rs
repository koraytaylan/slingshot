//! What a daemon is allowed to say about work it is not running.
//!
//! The table is the subject. Sling delivers at least once, so the same work
//! arrives as several physical jobs, and the tempting reading - that a requeue
//! means the job went back to waiting - is exactly the one that must be
//! refused. Running is where work stays while it is carried, however many times
//! it is carried, and a retry is metadata rather than a state.
//!
//! The other half is that endings do not move. A daemon whose most recently
//! delivered packet won would flip a finished job back to running whenever an
//! old event turned up late, so Succeeded and Failed accept nothing but
//! themselves and say so as a refusal rather than as a silent no-op.

use slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract;
use slingshot_domain::remote_job::{
    AgentJobIdentifier, AgentJobState, EventStreamCursor, FIRST_SEQUENCE, JobEventSequence,
    NO_ATTEMPTS_YET, RemoteJobFailure, RemoteJobObservation,
};

/// One physical job's name.
const JOB: &str = "sling-job-alpha";

/// How many attempts a job that has been retried once has had.
const SECOND_ATTEMPT: u64 = 2;

/// How far along a job that has reported once says it is.
const SOME_PROGRESS: u64 = 40;

/// How far along a job that has reported twice says it is.
const MORE_PROGRESS: u64 = 80;

/// A later position in one job's own event sequence.
const LATER_SEQUENCE: u64 = 5;

/// Returns what is known about a job that has started.
fn running() -> RemoteJobObservation {
    RemoteJobObservation::accepted()
        .advanced(
            AgentJobState::Running,
            JobEventSequence::of(SECOND_ATTEMPT),
            FIRST_SEQUENCE,
            NO_ATTEMPTS_YET,
        )
        .expect("starting is allowed")
}

#[test]
fn the_allowed_table_is_stated_once_and_covers_every_pair() {
    for held in AgentJobState::ALL {
        for next in AgentJobState::ALL {
            let permitted = held.may_become(*next);
            let expected = match held {
                AgentJobState::Queued => true,
                AgentJobState::Running => !matches!(next, AgentJobState::Queued),
                AgentJobState::Succeeded | AgentJobState::Failed => held == next,
            };
            assert_eq!(permitted, expected, "{held} to {next}");
        }
    }
    assert_eq!(AgentJobState::ALL.len(), 4, "four states, and the table covers every pair of them");
}

#[test]
fn a_physical_retry_is_the_same_work_running_and_never_work_waiting_again() {
    let held = running();
    let retried = held
        .advanced(
            AgentJobState::Running,
            JobEventSequence::of(LATER_SEQUENCE),
            SECOND_ATTEMPT,
            SOME_PROGRESS,
        )
        .expect("a requeue carries the same work");
    assert_eq!(retried.state, AgentJobState::Running);
    assert_eq!(retried.attempt, SECOND_ATTEMPT, "the retry shows up as metadata beside the state");
    assert_eq!(retried.progress, SOME_PROGRESS);
    assert_eq!(
        held.require_advanceable(AgentJobState::Queued, SECOND_ATTEMPT, SOME_PROGRESS),
        Err(RemoteJobFailure::RunningCannotRequeue)
    );
}

#[test]
fn attempts_and_progress_only_ever_increase() {
    let held = running()
        .advanced(
            AgentJobState::Running,
            JobEventSequence::of(LATER_SEQUENCE),
            SECOND_ATTEMPT,
            MORE_PROGRESS,
        )
        .expect("a report is allowed");
    assert_eq!(
        held.require_advanceable(AgentJobState::Running, FIRST_SEQUENCE, MORE_PROGRESS),
        Err(RemoteJobFailure::AttemptRegressed { held: SECOND_ATTEMPT, named: FIRST_SEQUENCE })
    );
    assert_eq!(
        held.require_advanceable(AgentJobState::Running, SECOND_ATTEMPT, SOME_PROGRESS),
        Err(RemoteJobFailure::ProgressRegressed { held: MORE_PROGRESS, named: SOME_PROGRESS })
    );
    assert!(
        held.require_advanceable(AgentJobState::Running, SECOND_ATTEMPT, MORE_PROGRESS).is_ok(),
        "an unchanged report is a replay, and a replay is ordinary"
    );
}

#[test]
fn an_ending_stays_the_ending_it_was() {
    for ending in [AgentJobState::Succeeded, AgentJobState::Failed] {
        let ended = running()
            .advanced(ending, JobEventSequence::of(LATER_SEQUENCE), SECOND_ATTEMPT, MORE_PROGRESS)
            .expect("finishing is allowed");
        assert!(ended.state.is_terminal());
        for next in AgentJobState::ALL {
            let permitted = ended.require_advanceable(*next, SECOND_ATTEMPT, MORE_PROGRESS).is_ok();
            assert_eq!(
                permitted,
                *next == ending,
                "{ending} accepts nothing but itself, and this said {next}"
            );
        }
        assert_eq!(
            ended.require_advanceable(AgentJobState::Running, SECOND_ATTEMPT, MORE_PROGRESS),
            Err(RemoteJobFailure::EndingIsFinal { from: ending, to: AgentJobState::Running }),
            "a late packet is refused rather than allowed to win"
        );
    }
}

#[test]
fn a_sequence_belongs_to_one_job_and_says_whether_anything_was_missed() {
    let held = JobEventSequence::first();
    assert_eq!(held.value(), FIRST_SEQUENCE);
    let next = JobEventSequence::of(FIRST_SEQUENCE + 1);
    assert!(next.follows(held) && next.immediately_follows(held));
    let ahead = JobEventSequence::of(LATER_SEQUENCE);
    assert!(
        ahead.follows(held) && !ahead.immediately_follows(held),
        "an event that follows but is not next means something in between was missed"
    );
    assert!(!held.follows(held), "a replayed sequence follows nothing");
    assert!(!held.follows(ahead));
}

#[test]
fn a_job_identifier_names_one_job_and_is_bounded() {
    let identifier = AgentJobIdentifier::new(JOB).expect("this names one job");
    assert_eq!(identifier.as_text(), JOB);
    assert_eq!(AgentJobIdentifier::new(""), Err(RemoteJobFailure::IdentifierEmpty));
    let allowed =
        AuthorAgentTransportContract::embedded().limit("maximum_sling_job_identifier_bytes");
    assert!(AgentJobIdentifier::new(&"j".repeat(allowed as usize)).is_ok());
    assert_eq!(
        AgentJobIdentifier::new(&"j".repeat(allowed as usize + 1)),
        Err(RemoteJobFailure::IdentifierTooLong { allowed, actual: allowed as usize + 1 })
    );
}

#[test]
fn a_durable_cursor_is_bounded_by_the_contract_that_issues_it() {
    let allowed =
        AuthorAgentTransportContract::embedded().limit("maximum_agent_operation_identifier_bytes");
    let cursor = EventStreamCursor::new("cursor-0001").expect("this is short");
    assert_eq!(cursor.as_text(), "cursor-0001");
    assert!(EventStreamCursor::new(&"c".repeat(allowed as usize)).is_ok());
    assert_eq!(
        EventStreamCursor::new(&"c".repeat(allowed as usize + 1)),
        Err(RemoteJobFailure::CursorTooLong { allowed, actual: allowed as usize + 1 })
    );
    assert!(
        EventStreamCursor::new("").is_ok(),
        "an empty cursor is a position the agent may legitimately issue, unlike an empty job name"
    );
}

#[test]
fn a_freshly_accepted_job_has_run_nothing_and_reported_nothing() {
    let accepted = RemoteJobObservation::accepted();
    assert_eq!(accepted.state, AgentJobState::Queued);
    assert_eq!(accepted.attempt, NO_ATTEMPTS_YET);
    assert_eq!(accepted.progress, NO_ATTEMPTS_YET);
    assert_eq!(accepted.applied_sequence, JobEventSequence::first());
    for next in AgentJobState::ALL {
        assert!(
            accepted.require_advanceable(*next, NO_ATTEMPTS_YET, NO_ATTEMPTS_YET).is_ok(),
            "a queued job may become anything, including finished without ever running"
        );
    }
}
