//! Assertions for looking inside a replication queue.
//!
//! The blocked state is a fact about the queue and lives on the page rather than
//! on every entry. Repeating it per row would invite two answers to one
//! question, and the row where they disagreed would be the one somebody was
//! looking at.

use serde_json::Value;
use slingshot_domain::command::command_identity::CommandContract;
use slingshot_domain::command::inspect_replication_queue::{
    InspectReplicationQueueCommand, InspectReplicationQueueResult, ReplicationQueueEntry,
};
use slingshot_domain::command::operational_listing::ListingResultFailure;
use slingshot_domain::command::platform_service_identity::{
    ReplicationAction, ReplicationQueueEntryIdentifier,
};
use slingshot_domain::command::repository_path::RepositoryPath;

/// Commands this test reads.
const COMMANDS: &str = include_str!("fixtures/commands/inspect_replication_queue/commands.jsonl");

/// Attempts one entry reported.
const ATTEMPTS: u64 = 4;

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

/// Returns one queue entry.
fn entry(identifier: &str, action: ReplicationAction, failed: bool) -> ReplicationQueueEntry {
    ReplicationQueueEntry {
        action,
        attempt_count: ATTEMPTS,
        content_path: RepositoryPath::parse("/content/example/en/report").expect("a legal path"),
        entry_identifier: ReplicationQueueEntryIdentifier::parse(identifier)
            .expect("a legal identifier"),
        last_failure_category: failed.then(|| "transport_unreachable".to_owned()),
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
            serde_json::from_str::<InspectReplicationQueueCommand>(document),
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
fn the_blocked_state_is_on_the_page_rather_than_on_every_entry() {
    let page = InspectReplicationQueueResult::new(
        true,
        vec![entry("e-1", ReplicationAction::Activate, true)],
        None,
    )
    .expect("a legal page");
    let written = serde_json::to_value(&page).expect("a page serializes");
    assert_eq!(written["blocked"], serde_json::Value::Bool(true));
    let row = &written["entries"][0];
    assert!(row.get("blocked").is_none(), "an entry carried the queue's blocked state");
}

#[test]
fn entries_are_strictly_ascending_and_bounded() {
    assert_eq!(
        InspectReplicationQueueResult::new(
            false,
            vec![
                entry("e-2", ReplicationAction::Activate, false),
                entry("e-1", ReplicationAction::Activate, false),
            ],
            None
        ),
        Err(ListingResultFailure::NotStrictlyAscending)
    );
    let bound =
        usize::try_from(CommandContract::embedded().limit("maximum_replication_queue_entries"))
            .expect("the bound fits");
    // The bound itself is a hundred thousand rows; building one past it is what
    // this proves, and the shorter page is proved by every other vector here.
    let beyond: Vec<ReplicationQueueEntry> = (0..=bound)
        .map(|index| entry(&format!("e-{index:06}"), ReplicationAction::Activate, false))
        .collect();
    assert_eq!(
        InspectReplicationQueueResult::new(false, beyond, None),
        Err(ListingResultFailure::TooManyRequested)
    );
}

#[test]
fn every_action_appears_and_an_absent_failure_is_omitted_rather_than_nulled() {
    for action in ReplicationAction::every() {
        let page =
            InspectReplicationQueueResult::new(false, vec![entry("e-1", action, false)], None)
                .expect("a legal page");
        let written = serde_json::to_string(&page).expect("a page serializes");
        assert!(!written.contains("last_failure_category"), "an absent failure was serialized");
        let read: InspectReplicationQueueResult =
            serde_json::from_str(&written).expect("a page parses");
        assert_eq!(read, page);
    }
}
