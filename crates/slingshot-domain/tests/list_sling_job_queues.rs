//! Assertions for listing Sling job queues.
//!
//! The two counts stay separate because they answer different questions: many
//! active jobs is busy, many queued and none active is stuck, and one total
//! would make those indistinguishable.

use serde_json::Value;
use slingshot_domain::command::command_identity::CommandContract;
use slingshot_domain::command::list_sling_job_queues::{
    ListSlingJobQueuesCommand, ListSlingJobQueuesResult, SlingJobQueueMatch,
};
use slingshot_domain::command::operational_listing::ListingResultFailure;
use slingshot_domain::command::process_identity::{SlingJobQueueName, SlingJobQueueState};

/// Commands this test reads.
const COMMANDS: &str = include_str!("fixtures/commands/list_sling_job_queues/commands.jsonl");

/// Jobs one queue reported running.
const ACTIVE: u64 = 2;

/// Jobs one queue reported waiting.
const QUEUED: u64 = 17;

/// Reads one row's string member.
fn text<'row>(row: &'row Value, member: &str) -> &'row str {
    row[member].as_str().unwrap_or_else(|| panic!("{member} is a string in {row}"))
}

/// Returns every row of one fixture.
fn rows(fixture: &str) -> Vec<Value> {
    fixture
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("every fixture line is one object"))
        .collect()
}

/// Returns one queue row.
fn matched(name: &str, state: SlingJobQueueState) -> SlingJobQueueMatch {
    SlingJobQueueMatch::new(
        ACTIVE,
        SlingJobQueueName::parse(name).expect("a legal queue name"),
        QUEUED,
        state,
    )
    .expect("a legal row")
}

#[test]
fn every_command_vector_parses_exactly_as_the_fixture_says() {
    let vectors = rows(COMMANDS);
    assert!(vectors.len() >= 3, "every document shape and every refusal");
    for row in &vectors {
        let document = text(row, "document");
        let note = text(row, "note");
        match (
            row["accepted"].as_bool(),
            serde_json::from_str::<ListSlingJobQueuesCommand>(document),
        ) {
            (Some(true), Ok(parsed)) => {
                assert_eq!(
                    serde_json::to_string(&parsed).expect("a command serializes"),
                    document,
                    "{note}: rewritten differently"
                );
            }
            (Some(false), Err(_)) => (),
            (_, parsed) => panic!("{note}: the command answered {parsed:?}"),
        }
    }
}

#[test]
fn both_counts_are_reported_and_neither_is_the_other() {
    let queue = matched("Granite Workflow Queue", SlingJobQueueState::Running);
    assert_eq!(queue.active_job_count, ACTIVE);
    assert_eq!(queue.queued_job_count, QUEUED);
    let written = serde_json::to_string(&queue).expect("a row serializes");
    assert!(written.contains("active_job_count"));
    assert!(written.contains("queued_job_count"));
}

#[test]
fn a_count_is_accepted_at_its_bound_and_refused_one_past_it() {
    let bound = CommandContract::embedded().limit("maximum_operational_candidate_records");
    let name = SlingJobQueueName::parse("Granite Workflow Queue").expect("a legal queue name");
    assert!(
        SlingJobQueueMatch::new(bound, name.clone(), 0, SlingJobQueueState::Running).is_ok(),
        "the bound itself was refused"
    );
    assert_eq!(
        SlingJobQueueMatch::new(bound + 1, name.clone(), 0, SlingJobQueueState::Running),
        Err(ListingResultFailure::TooManyRequested)
    );
    assert_eq!(
        SlingJobQueueMatch::new(0, name, bound + 1, SlingJobQueueState::Running),
        Err(ListingResultFailure::TooManyRequested)
    );
}

#[test]
fn rows_are_strictly_ascending_by_queue_name() {
    assert!(
        ListSlingJobQueuesResult::new(
            vec![
                matched("A queue", SlingJobQueueState::Running),
                matched("B queue", SlingJobQueueState::Suspended),
            ],
            None
        )
        .is_ok()
    );
    assert_eq!(
        ListSlingJobQueuesResult::new(
            vec![
                matched("B queue", SlingJobQueueState::Running),
                matched("A queue", SlingJobQueueState::Running),
            ],
            None
        ),
        Err(ListingResultFailure::NotStrictlyAscending)
    );
}
