//! Assertions for emptying a replication queue.
//!
//! The expectation guard is what makes this safe to run under pressure: it is
//! checked before anything is removed, so a mismatch is a refusal that proves no
//! effect, and the caller looks again rather than discovering afterwards that it
//! emptied more than it saw.

use serde_json::Value;
use slingshot_domain::command::command_identity::CommandContract;
use slingshot_domain::command::flush_replication_queue::{
    FlushReplicationQueueCommand, FlushReplicationQueueFailure, FlushReplicationQueueRefusal,
    FlushReplicationQueueResult,
};
use slingshot_domain::command::platform_service_identity::ReplicationAgentIdentifier;
use slingshot_domain::command::resource_mutation::MutationResultFailure;

/// Commands this test reads.
const COMMANDS: &str = include_str!("fixtures/commands/flush_replication_queue/commands.jsonl");

/// Failures this test reads.
const FAILURES: &str = include_str!("fixtures/commands/flush_replication_queue/failures.jsonl");

/// Agent every vector addresses.
const AGENT: &str = "publish";

/// Entries one caller believed were queued.
const EXPECTED: u64 = 12;

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

/// Returns one agent identifier.
fn agent(value: &str) -> ReplicationAgentIdentifier {
    ReplicationAgentIdentifier::parse(value).expect("a legal identifier")
}

/// Returns one request over its expectation.
fn command(expected_entry_count: Option<u64>) -> FlushReplicationQueueCommand {
    FlushReplicationQueueCommand { agent_identifier: agent(AGENT), expected_entry_count }
}

#[test]
fn every_command_vector_parses_exactly_as_the_fixture_says() {
    let vectors = rows(COMMANDS);
    assert!(vectors.len() >= 4, "every document shape and every refusal");
    for row in &vectors {
        let document = text(row, "document");
        let note = text(row, "note");
        match (
            row["accepted"].as_bool(),
            serde_json::from_str::<FlushReplicationQueueCommand>(document),
        ) {
            (Some(true), Ok(parsed)) => {
                assert_eq!(
                    serde_json::to_string(&parsed).expect("a command serializes"),
                    document,
                    "{note}: rewritten differently"
                );
                assert_eq!(parsed.require_usable(), Ok(()), "{note}: refused as unusable");
            }
            (Some(false), Err(_)) => (),
            (_, parsed) => panic!("{note}: the command answered {parsed:?}"),
        }
    }
}

#[test]
fn a_mismatch_belongs_only_to_a_request_that_stated_an_expectation() {
    let refusal = FlushReplicationQueueRefusal {
        agent_identifier: agent(AGENT),
        failure: FlushReplicationQueueFailure::QueueExpectationMismatch,
    };
    assert!(refusal.proves_no_effect(), "the guard's refusal did not prove no effect");
    assert_eq!(refusal.require_answers(&command(Some(EXPECTED))), Ok(()));
    assert_eq!(
        refusal.require_answers(&command(None)),
        Err(MutationResultFailure::NotThisRequest),
        "a request that stated no expectation was answered with a mismatch"
    );
}

#[test]
fn a_flush_cannot_remove_more_than_the_request_expected() {
    let answered = FlushReplicationQueueResult {
        agent_identifier: agent(AGENT),
        removed_entry_count: EXPECTED + 1,
    };
    assert_eq!(
        answered.require_answers(&command(Some(EXPECTED))),
        Err(MutationResultFailure::NotThisRequest)
    );
    let exact = FlushReplicationQueueResult {
        agent_identifier: agent(AGENT),
        removed_entry_count: EXPECTED,
    };
    assert_eq!(exact.require_answers(&command(Some(EXPECTED))), Ok(()));
    assert_eq!(exact.require_answers(&command(None)), Ok(()));
}

#[test]
fn both_counts_are_accepted_at_the_queue_bound_and_refused_one_past_it() {
    let bound = CommandContract::embedded().limit("maximum_replication_queue_entries");
    assert_eq!(command(Some(bound)).require_usable(), Ok(()));
    assert_eq!(
        command(Some(bound + 1)).require_usable(),
        Err(MutationResultFailure::CountTooLarge)
    );
    let beyond = FlushReplicationQueueResult {
        agent_identifier: agent(AGENT),
        removed_entry_count: bound + 1,
    };
    assert_eq!(beyond.require_answers(&command(None)), Err(MutationResultFailure::CountTooLarge));
}

#[test]
fn a_result_answers_only_the_request_that_named_its_agent() {
    let elsewhere =
        FlushReplicationQueueResult { agent_identifier: agent("flush"), removed_entry_count: 0 };
    assert_eq!(
        elsewhere.require_answers(&command(None)),
        Err(MutationResultFailure::NotThisRequest)
    );
}

#[test]
fn every_failure_document_carries_its_members_and_says_what_it_proves() {
    let vectors = rows(FAILURES);
    assert_eq!(vectors.len(), 5, "one for each category this command allows");
    for row in &vectors {
        let note = text(row, "note");
        let document = text(row, "document");
        let refusal: FlushReplicationQueueRefusal =
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
        serde_json::from_str::<FlushReplicationQueueRefusal>(
            r#"{"agent_identifier":"publish","failure":"agent_not_found","extra":1}"#
        )
        .is_err(),
        "a surplus member was accepted"
    );
}
