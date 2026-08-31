//! Assertions for trying one queued entry again.
//!
//! The result says whether the entry was actually resubmitted, because an entry
//! that had already left the queue between looking and acting is a different
//! outcome from one that was retried - and a caller that could not tell them
//! apart would retry the wrong thing next.

use serde_json::Value;
use slingshot_domain::command::platform_service_identity::{
    ReplicationAgentIdentifier, ReplicationQueueEntryIdentifier,
};
use slingshot_domain::command::resource_mutation::MutationResultFailure;
use slingshot_domain::command::retry_replication_queue_entry::{
    RetryReplicationQueueEntryCommand, RetryReplicationQueueEntryFailure,
    RetryReplicationQueueEntryRefusal, RetryReplicationQueueEntryResult,
};

/// Commands this test reads.
const COMMANDS: &str =
    include_str!("fixtures/commands/retry_replication_queue_entry/commands.jsonl");

/// Failures this test reads.
const FAILURES: &str =
    include_str!("fixtures/commands/retry_replication_queue_entry/failures.jsonl");

/// Agent every vector addresses.
const AGENT: &str = "publish";

/// Entry every vector retries.
const ENTRY: &str = "queue-entry-1";

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

/// Returns the request every assertion answers.
fn command() -> RetryReplicationQueueEntryCommand {
    RetryReplicationQueueEntryCommand {
        agent_identifier: ReplicationAgentIdentifier::parse(AGENT).expect("a legal identifier"),
        entry_identifier: ReplicationQueueEntryIdentifier::parse(ENTRY)
            .expect("a legal identifier"),
    }
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
            serde_json::from_str::<RetryReplicationQueueEntryCommand>(document),
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
fn the_result_says_whether_the_entry_was_actually_resubmitted() {
    for resubmitted in [true, false] {
        let answered = RetryReplicationQueueEntryResult {
            agent_identifier: ReplicationAgentIdentifier::parse(AGENT).expect("a legal identifier"),
            entry_identifier: ReplicationQueueEntryIdentifier::parse(ENTRY)
                .expect("a legal identifier"),
            resubmitted,
        };
        assert_eq!(answered.require_answers(&command()), Ok(()));
        let written = serde_json::to_string(&answered).expect("a result serializes");
        assert!(written.contains("resubmitted"), "the outcome member is not optional");
    }
}

#[test]
fn a_result_and_a_refusal_answer_only_the_request_that_named_the_pair() {
    let elsewhere = RetryReplicationQueueEntryResult {
        agent_identifier: ReplicationAgentIdentifier::parse("flush").expect("a legal identifier"),
        entry_identifier: ReplicationQueueEntryIdentifier::parse(ENTRY)
            .expect("a legal identifier"),
        resubmitted: true,
    };
    assert_eq!(elsewhere.require_answers(&command()), Err(MutationResultFailure::NotThisRequest));
    let refusal = RetryReplicationQueueEntryRefusal {
        agent_identifier: ReplicationAgentIdentifier::parse(AGENT).expect("a legal identifier"),
        entry_identifier: ReplicationQueueEntryIdentifier::parse("another-entry")
            .expect("a legal identifier"),
        failure: RetryReplicationQueueEntryFailure::EntryNotFound,
    };
    assert_eq!(refusal.require_answers(&command()), Err(MutationResultFailure::NotThisRequest));
}

#[test]
fn every_failure_document_carries_its_members_and_says_what_it_proves() {
    let vectors = rows(FAILURES);
    assert_eq!(vectors.len(), 5, "one for each category this command allows");
    for row in &vectors {
        let note = text(row, "note");
        let document = text(row, "document");
        let refusal: RetryReplicationQueueEntryRefusal =
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
        serde_json::from_str::<RetryReplicationQueueEntryRefusal>(r#"{"agent_identifier":"publish","entry_identifier":"queue-entry-1","failure":"entry_not_found","extra":1}"#).is_err(),
        "a surplus member was accepted"
    );
}
