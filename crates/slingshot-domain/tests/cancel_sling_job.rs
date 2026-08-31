//! Assertions for cancelling a Sling job.
//!
//! A cancellation that succeeded and left the job still cancellable is two
//! answers at once, so a result observing `queued` or `active` is refused. A job
//! that had already ended is a refusal rather than a success, so a caller can
//! tell "I stopped it" from "it had already stopped".

use serde_json::Value;
use slingshot_domain::command::cancel_sling_job::{
    CancelSlingJobCommand, CancelSlingJobFailure, CancelSlingJobRefusal, CancelSlingJobResult,
};
use slingshot_domain::command::process_identity::{SlingJobIdentifier, SlingJobState};
use slingshot_domain::command::resource_mutation::MutationResultFailure;

/// Commands this test reads.
const COMMANDS: &str = include_str!("fixtures/commands/cancel_sling_job/commands.jsonl");

/// Failures this test reads.
const FAILURES: &str = include_str!("fixtures/commands/cancel_sling_job/failures.jsonl");

/// Job every vector cancels.
const JOB: &str = "2024/01/01/example-job-1";

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

/// Returns the request every result assertion answers.
fn command() -> CancelSlingJobCommand {
    CancelSlingJobCommand { job_identifier: job(JOB) }
}

#[test]
fn every_command_vector_parses_exactly_as_the_fixture_says() {
    let vectors = rows(COMMANDS);
    assert!(vectors.len() >= 2, "every document shape and every refusal");
    for row in &vectors {
        let document = text(row, "document");
        let note = text(row, "note");
        match (row["accepted"].as_bool(), serde_json::from_str::<CancelSlingJobCommand>(document)) {
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
fn a_cancellation_cannot_answer_with_a_job_that_is_still_cancellable() {
    for still_running in [SlingJobState::Queued, SlingJobState::Active] {
        let answered =
            CancelSlingJobResult { job_identifier: job(JOB), observed_state: still_running };
        assert_eq!(
            answered.require_answers(&command()),
            Err(MutationResultFailure::NotThisRequest),
            "{still_running:?} was accepted as the outcome of a cancellation"
        );
    }
    for ended in [SlingJobState::Cancelled, SlingJobState::Dropped, SlingJobState::Succeeded] {
        let answered = CancelSlingJobResult { job_identifier: job(JOB), observed_state: ended };
        assert_eq!(answered.require_answers(&command()), Ok(()));
    }
}

#[test]
fn a_job_that_had_already_ended_is_a_refusal_rather_than_a_success() {
    let refusal = CancelSlingJobRefusal {
        failure: CancelSlingJobFailure::JobNotCancellable,
        job_identifier: job(JOB),
    };
    assert!(refusal.proves_no_effect());
    assert_eq!(refusal.require_answers(&command()), Ok(()));
}

#[test]
fn a_result_answers_only_the_request_that_named_its_job() {
    let elsewhere = CancelSlingJobResult {
        job_identifier: job("another-job"),
        observed_state: SlingJobState::Cancelled,
    };
    assert_eq!(elsewhere.require_answers(&command()), Err(MutationResultFailure::NotThisRequest));
}

#[test]
fn every_failure_document_carries_its_members_and_says_what_it_proves() {
    let vectors = rows(FAILURES);
    assert_eq!(vectors.len(), 4, "one for each category this command allows");
    for row in &vectors {
        let note = text(row, "note");
        let document = text(row, "document");
        let refusal: CancelSlingJobRefusal =
            serde_json::from_str(document).unwrap_or_else(|failure| panic!("{note}: {failure}"));
        assert_eq!(
            serde_json::to_string(&refusal).expect("a refusal serializes"),
            document,
            "{note}: rewritten differently"
        );
        assert_eq!(
            refusal.proves_no_effect(),
            row["proves_no_effect"].as_bool().expect("a verdict"),
            "{note}"
        );
    }
    assert!(
        serde_json::from_str::<CancelSlingJobRefusal>(
            r#"{"failure":"job_not_found","job_identifier":"2024/01/01/example-job-1","extra":1}"#
        )
        .is_err(),
        "a surplus member was accepted"
    );
}
