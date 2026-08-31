//! Assertions for listing the effective resource mapping.
//!
//! One of these is about what the command deliberately does not take. A
//! pattern-shaped argument would invite a caller to believe a pattern had been
//! matched when it had only been listed, and that distinction is the whole
//! question a mapping problem is about - so a document carrying one is refused.

use serde_json::Value;
use slingshot_domain::command::list_resource_mappings::{
    ListResourceMappingsCommand, ListResourceMappingsResult,
};
use slingshot_domain::command::operational_listing::ListingResultFailure;
use slingshot_domain::command::repository_path::RepositoryPath;
use slingshot_domain::command::resource_mapping_entry::{
    ResourceMappingEntry, ResourceMappingKind, ResourceMappingPattern,
};
use slingshot_domain::command::result_window::ResultWindow;

/// Commands this test reads.
const COMMANDS: &str = include_str!("fixtures/commands/list_resource_mappings/commands.jsonl");

/// A status a redirecting entry answers with.
const MOVED_PERMANENTLY: u16 = 301;

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

/// Returns one legal path.
fn path(value: &str) -> RepositoryPath {
    RepositoryPath::parse(value).expect("a legal path")
}

/// Returns one entry at `entry_path`.
fn entry(entry_path: &str, kind: ResourceMappingKind) -> ResourceMappingEntry {
    ResourceMappingEntry::new(
        path(entry_path),
        kind,
        ResourceMappingPattern::parse("^example\\.test/").expect("a legal pattern"),
        vec!["/content/example/".to_owned()],
        kind.redirects().then_some(MOVED_PERMANENTLY),
    )
    .expect("a legal entry")
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
            serde_json::from_str::<ListResourceMappingsCommand>(document),
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
fn this_command_takes_no_filter_and_refuses_one() {
    assert!(
        serde_json::from_str::<ListResourceMappingsCommand>(r#"{"pattern":"example.test"}"#)
            .is_err(),
        "a filter this command does not take was accepted"
    );
    assert_eq!(
        ListResourceMappingsCommand { result_window: None }.resolved_window(),
        ResultWindow::default()
    );
}

#[test]
fn entries_are_strictly_ascending_by_entry_address() {
    assert!(
        ListResourceMappingsResult::new(
            vec![
                entry("/etc/map/https/a.example.test", ResourceMappingKind::Map),
                entry("/etc/map/https/b.example.test", ResourceMappingKind::Redirect),
            ],
            None
        )
        .is_ok()
    );
    assert_eq!(
        ListResourceMappingsResult::new(
            vec![
                entry("/etc/map/https/b.example.test", ResourceMappingKind::Map),
                entry("/etc/map/https/a.example.test", ResourceMappingKind::Map),
            ],
            None
        ),
        Err(ListingResultFailure::NotStrictlyAscending)
    );
}

#[test]
fn a_page_round_trips_with_a_redirect_and_a_map_side_by_side() {
    let page = ListResourceMappingsResult::new(
        vec![
            entry("/etc/map/https/a.example.test", ResourceMappingKind::Map),
            entry("/etc/map/https/b.example.test", ResourceMappingKind::Redirect),
        ],
        None,
    )
    .expect("a legal page");
    let written = serde_json::to_string(&page).expect("a page serializes");
    let read: ListResourceMappingsResult = serde_json::from_str(&written).expect("a page parses");
    assert_eq!(read, page);
    assert!(written.contains("status_code"), "the redirect lost its status");
}
