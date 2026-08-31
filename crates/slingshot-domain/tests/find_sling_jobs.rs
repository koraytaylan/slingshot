//! Assertions for finding the jobs behind a queue's numbers.

use serde_json::Value;
use slingshot_domain::command::find_sling_jobs::{
    FindSlingJobsCommand, FindSlingJobsResult, SlingJobMatch,
};
use slingshot_domain::command::operational_listing::ListingResultFailure;
use slingshot_domain::command::process_identity::{
    RequestedSlingJobStates, SlingJobIdentifier, SlingJobState, SlingJobTopic,
};

/// Commands this test reads.
const COMMANDS: &str = include_str!("fixtures/commands/find_sling_jobs/commands.jsonl");

/// Retries one job reported.
const RETRIES: u64 = 3;

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

/// Returns one job row.
fn matched(identifier: &str, topic: &str, state: SlingJobState) -> SlingJobMatch {
    SlingJobMatch {
        job_identifier: SlingJobIdentifier::parse(identifier).expect("a legal identifier"),
        queue_name: None,
        retry_count: RETRIES,
        state,
        topic: SlingJobTopic::parse(topic).expect("a legal topic"),
    }
}

/// Returns one request over its states and an optional topic.
fn command(states: Vec<SlingJobState>, topic: Option<&str>) -> FindSlingJobsCommand {
    FindSlingJobsCommand {
        result_window: None,
        states: RequestedSlingJobStates::new(states).expect("a legal set"),
        topic: topic.map(|topic| SlingJobTopic::parse(topic).expect("a legal topic")),
    }
}

#[test]
fn every_command_vector_parses_exactly_as_the_fixture_says() {
    let vectors = rows(COMMANDS);
    assert!(vectors.len() >= 6, "every document shape and every refusal");
    for row in &vectors {
        let document = text(row, "document");
        let note = text(row, "note");
        match (row["accepted"].as_bool(), serde_json::from_str::<FindSlingJobsCommand>(document)) {
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
fn the_state_set_is_required_and_has_no_default() {
    assert!(
        serde_json::from_str::<FindSlingJobsCommand>("{}").is_err(),
        "a request with no state set was given one"
    );
}

#[test]
fn a_match_outside_the_requested_topic_or_states_is_refused() {
    let page = FindSlingJobsResult::new(
        vec![matched("j-1", "com/example/other", SlingJobState::Error)],
        None,
    )
    .expect("a legal page");
    assert_eq!(page.require_answers(&command(vec![SlingJobState::Error], None)), Ok(()));
    assert_eq!(
        page.require_answers(&command(vec![SlingJobState::Error], Some("com/example/reindex"))),
        Err(ListingResultFailure::NotThisRequest)
    );
    assert_eq!(
        page.require_answers(&command(vec![SlingJobState::Queued], None)),
        Err(ListingResultFailure::NotThisRequest)
    );
}

#[test]
fn an_absent_queue_name_is_omitted_rather_than_nulled() {
    let written =
        serde_json::to_string(&matched("j-1", "com/example/reindex", SlingJobState::Queued))
            .expect("a match serializes");
    assert!(!written.contains("queue_name"), "an absent queue name was serialized");
}

#[test]
fn rows_are_strictly_ascending_by_job_identifier() {
    assert_eq!(
        FindSlingJobsResult::new(
            vec![
                matched("j-2", "com/example/reindex", SlingJobState::Error),
                matched("j-1", "com/example/reindex", SlingJobState::Error),
            ],
            None
        ),
        Err(ListingResultFailure::NotStrictlyAscending)
    );
}
