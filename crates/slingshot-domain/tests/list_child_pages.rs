//! Assertions for listing the pages directly below one anchor.
//!
//! The membership table is the point. "Directly below" is parent equality and
//! not a prefix comparison, which is exactly the distinction a grandchild and a
//! sibling whose name begins the same would each get wrong.

use serde_json::Value;
use slingshot_domain::command::list_child_pages::{ListChildPagesCommand, ListChildPagesResult};
use slingshot_domain::command::query_paths::DiscoveryResultFailure;
use slingshot_domain::command::repository_path::RepositoryPath;
use slingshot_domain::command::result_window::ResultWindow;

/// Commands this test reads.
const COMMANDS: &str = include_str!("fixtures/commands/list_child_pages/commands.jsonl");

/// Membership vectors this test reads.
const MEMBERSHIP: &str = include_str!("fixtures/commands/list_child_pages/membership.jsonl");

/// Anchor every membership vector is asked about.
const ANCHOR: &str = "/content/example/en";

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

/// Returns one page match document over `page`.
fn matched(page: &str) -> Value {
    serde_json::json!({"repository_path": page})
}

/// Returns one result document carrying `matches`.
fn result(matches: Vec<Value>) -> String {
    serde_json::to_string(&serde_json::json!({"matches": matches})).expect("a document")
}

#[test]
fn every_command_vector_parses_exactly_as_the_fixture_says() {
    let vectors = rows(COMMANDS);
    assert!(vectors.len() >= 7, "both window forms, the root, and every refusal");
    for row in &vectors {
        let document = text(row, "document");
        let note = text(row, "note");
        match (row["accepted"].as_bool(), serde_json::from_str::<ListChildPagesCommand>(document)) {
            (Some(true), Ok(parsed)) => assert_eq!(
                serde_json::to_string(&parsed).expect("a command serializes"),
                document,
                "{note}: rewritten differently"
            ),
            (Some(false), Err(_)) => (),
            (_, parsed) => panic!("{note}: the command answered {parsed:?}"),
        }
    }
}

#[test]
fn directly_below_is_parent_equality_rather_than_a_prefix_comparison() {
    let asked = ListChildPagesCommand { result_window: None, root_path: path(ANCHOR) };
    let vectors = rows(MEMBERSHIP);
    assert!(vectors.len() >= 5, "a child, a grandchild, a lookalike, the anchor, and its parent");
    for row in &vectors {
        let note = text(row, "note");
        let candidate = path(text(row, "candidate"));
        assert_eq!(
            asked.is_immediate_child(&candidate),
            row["immediate"].as_bool().expect("a verdict"),
            "{note}"
        );
    }
}

#[test]
fn an_absent_window_resolves_to_the_default_one() {
    let asked = ListChildPagesCommand { result_window: None, root_path: path(ANCHOR) };
    assert_eq!(asked.resolved_window(), ResultWindow::default());
}

#[test]
fn an_empty_page_and_a_strictly_ascending_page_both_round_trip() {
    for matches in [
        Vec::new(),
        vec![matched("/content/example/en/a")],
        vec![matched("/content/example/en/a"), matched("/content/example/en/b")],
    ] {
        let document = result(matches);
        let parsed: ListChildPagesResult = serde_json::from_str(&document).expect("a legal result");
        assert_eq!(serde_json::to_string(&parsed).expect("a result serializes"), document);
    }
}

#[test]
fn a_repeated_or_descending_match_is_refused() {
    for matches in [
        vec![matched("/content/example/en/a"), matched("/content/example/en/a")],
        vec![matched("/content/example/en/b"), matched("/content/example/en/a")],
    ] {
        assert!(
            serde_json::from_str::<ListChildPagesResult>(&result(matches)).is_err(),
            "an unordered page was accepted"
        );
    }
}

#[test]
fn a_match_that_is_not_an_immediate_child_is_refused_by_request_context() {
    let asked = ListChildPagesCommand { result_window: None, root_path: path(ANCHOR) };
    let below: ListChildPagesResult =
        serde_json::from_str(&result(vec![matched("/content/example/en/report")]))
            .expect("a legal result");
    assert_eq!(below.require_answers(&asked), Ok(()));
    for outside in ["/content/example/en/report/deeper", "/content/example/enterprise"] {
        let answered: ListChildPagesResult =
            serde_json::from_str(&result(vec![matched(outside)])).expect("a legal result");
        assert_eq!(
            answered.require_answers(&asked),
            Err(DiscoveryResultFailure::NotThisRequest),
            "{outside} was accepted as an immediate child"
        );
    }
}
