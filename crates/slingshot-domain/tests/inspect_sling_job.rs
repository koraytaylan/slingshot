//! Assertions for inspecting one Sling job.
//!
//! The structural assertion is the important one: the result type has no member
//! that could hold a property value. A deployment puts whatever it likes in a
//! job's properties, and this command has made no judgement about which of those
//! would be safe to read, so it reads none.

use serde_json::Value;
use slingshot_domain::command::command_identity::CommandContract;
use slingshot_domain::command::inspect_sling_job::{
    InspectSlingJobCommand, InspectSlingJobFailure, InspectSlingJobRefusal, InspectSlingJobResult,
};
use slingshot_domain::command::operational_listing::ListingResultFailure;
use slingshot_domain::command::process_identity::{
    SlingJobIdentifier, SlingJobState, SlingJobTopic,
};
use slingshot_domain::command::repository_path::PropertyName;

/// Commands this test reads.
const COMMANDS: &str = include_str!("fixtures/commands/inspect_sling_job/commands.jsonl");

/// Failures this test reads.
const FAILURES: &str = include_str!("fixtures/commands/inspect_sling_job/failures.jsonl");

/// Job every vector inspects.
const JOB: &str = "2024/01/01/example-job-1";

/// Retries one job reported.
const RETRIES: u64 = 3;

/// Retries that job is allowed.
const MAXIMUM_RETRIES: u64 = 10;

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

/// Returns one job identifier.
fn job(value: &str) -> SlingJobIdentifier {
    SlingJobIdentifier::parse(value).expect("a legal identifier")
}

/// Returns one inspection carrying `keys`.
fn result(keys: Vec<&str>) -> Result<InspectSlingJobResult, ListingResultFailure> {
    InspectSlingJobResult::new(
        job(JOB),
        MAXIMUM_RETRIES,
        keys.into_iter()
            .map(|key| PropertyName::parse(key).expect("a legal property name"))
            .collect(),
        None,
        RETRIES,
        SlingJobState::Error,
        SlingJobTopic::parse("com/example/jobs/reindex").expect("a legal topic"),
    )
}

/// Returns the request every result assertion answers.
fn command() -> InspectSlingJobCommand {
    InspectSlingJobCommand { job_identifier: job(JOB) }
}

#[test]
fn every_command_vector_parses_exactly_as_the_fixture_says() {
    let vectors = rows(COMMANDS);
    assert!(vectors.len() >= 3, "every document shape and every refusal");
    for row in &vectors {
        let document = text(row, "document");
        let note = text(row, "note");
        match (row["accepted"].as_bool(), serde_json::from_str::<InspectSlingJobCommand>(document))
        {
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
fn no_member_of_the_result_could_carry_a_property_value() {
    let answered = result(vec!["slingshot:path"]).expect("a legal inspection");
    let written = serde_json::to_value(&answered).expect("an inspection serializes");
    let members: Vec<&str> =
        written.as_object().expect("an object").keys().map(String::as_str).collect();
    assert_eq!(
        members,
        vec![
            "job_identifier",
            "maximum_retry_count",
            "property_keys",
            "retry_count",
            "state",
            "topic",
        ],
        "an inspection grew a member that could hold a property value"
    );
}

#[test]
fn both_retry_counts_are_reported_because_one_without_the_other_says_nothing() {
    let answered = result(Vec::new()).expect("a legal inspection");
    assert_eq!(answered.retry_count, RETRIES);
    assert_eq!(answered.maximum_retry_count, MAXIMUM_RETRIES);
}

#[test]
fn keys_are_ascending_distinct_and_bounded() {
    assert_eq!(result(vec!["b", "a"]), Err(ListingResultFailure::NotStrictlyAscending));
    assert_eq!(result(vec!["a", "a"]), Err(ListingResultFailure::NotStrictlyAscending));
    let bound =
        usize::try_from(CommandContract::embedded().limit("maximum_sling_job_property_keys"))
            .expect("the bound fits");
    let keys: Vec<String> = (0..=bound).map(|index| format!("k{index:06}")).collect();
    let exact: Vec<&str> = keys[..bound].iter().map(String::as_str).collect();
    assert!(result(exact).is_ok(), "the bound itself was refused");
    let beyond: Vec<&str> = keys.iter().map(String::as_str).collect();
    assert_eq!(result(beyond), Err(ListingResultFailure::TooManyRequested));
}

#[test]
fn a_result_and_a_refusal_answer_only_the_request_that_named_the_job() {
    assert_eq!(result(Vec::new()).expect("a legal inspection").require_answers(&command()), Ok(()));
    let elsewhere = InspectSlingJobResult::new(
        job("another-job"),
        MAXIMUM_RETRIES,
        Vec::new(),
        None,
        RETRIES,
        SlingJobState::Error,
        SlingJobTopic::parse("com/example/jobs/reindex").expect("a legal topic"),
    )
    .expect("a legal inspection");
    assert_eq!(elsewhere.require_answers(&command()), Err(ListingResultFailure::NotThisRequest));
    let refusal = InspectSlingJobRefusal {
        failure: InspectSlingJobFailure::JobNotFound,
        job_identifier: job(JOB),
    };
    assert_eq!(refusal.require_answers(&command()), Ok(()));
}

#[test]
fn every_failure_document_round_trips() {
    let vectors = rows(FAILURES);
    assert_eq!(vectors.len(), 3, "one for each category this command allows");
    for row in &vectors {
        let note = text(row, "note");
        let document = text(row, "document");
        let refusal: InspectSlingJobRefusal =
            serde_json::from_str(document).unwrap_or_else(|failure| panic!("{note}: {failure}"));
        assert_eq!(
            serde_json::to_string(&refusal).expect("a refusal serializes"),
            document,
            "{note}: rewritten differently"
        );
    }
}
