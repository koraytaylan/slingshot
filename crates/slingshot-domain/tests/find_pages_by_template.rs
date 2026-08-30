//! Finding pages by template, proved to compare addresses rather than strings.
//!
//! The stored value has to validate as a repository path before it is compared,
//! so a trailing slash, a doubled separator, and a same-name-sibling suffix are
//! all different templates - or not templates at all - rather than near misses
//! that a string comparison might have let through.

use serde_json::Value;
use slingshot_domain::command::find_pages_by_template::{
    FindPagesByTemplateCommand, FindPagesByTemplateResult, PAGE_TEMPLATE_PROPERTY,
    TEMPLATE_PROPERTY_TYPES,
};
use slingshot_domain::command::find_pages_containing_phrase::PageMatch;
use slingshot_domain::command::query_paths::DiscoveryResultFailure;
use slingshot_domain::command::repository_path::RepositoryPath;

/// Commands this test reads.
const COMMANDS: &str = include_str!("fixtures/commands/find_pages_by_template/commands.jsonl");

/// Matching vectors this test reads.
const MATCHING: &str = include_str!("fixtures/commands/find_pages_by_template/matching.jsonl");

/// Cardinality vectors this test reads.
const CARDINALITIES: &str =
    include_str!("fixtures/commands/find_pages_by_template/cardinalities.jsonl");

/// Results this test reads.
const RESULTS: &str = include_str!("fixtures/commands/find_pages_by_template/results.jsonl");

/// Every refusal the fixtures can name, beside the variant that produces it.
const DECLARED_REFUSALS: &[(&str, DiscoveryResultFailure)] = &[
    ("NotStrictlyAscending", DiscoveryResultFailure::NotStrictlyAscending),
    ("NotThisRequest", DiscoveryResultFailure::NotThisRequest),
];

/// Name the fixtures give to the refusals the closed object makes on its own.
const CLOSED_OBJECT: &str = "ClosedObject";

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

/// Checks one accept-or-refuse vector against the type it names.
fn check<Parsed: serde::de::DeserializeOwned + std::fmt::Debug>(row: &Value) {
    let document = text(row, "document");
    let note = text(row, "note");
    let declared = text(row, "reason");
    match (row["accepted"].as_bool(), serde_json::from_str::<Parsed>(document)) {
        (Some(true), Ok(_)) => (),
        (Some(false), Err(failure)) => {
            let rendered = failure.to_string();
            let known: Vec<String> =
                DECLARED_REFUSALS.iter().map(|(_, failure)| failure.to_string()).collect();
            if declared == CLOSED_OBJECT {
                assert!(!known.contains(&rendered), "{note}: {rendered}");
            } else {
                let expected = DECLARED_REFUSALS
                    .iter()
                    .find(|(name, _)| *name == declared)
                    .map(|(_, failure)| failure.to_string())
                    .unwrap_or_else(|| panic!("{note}: unknown refusal {declared}"));
                assert!(rendered.contains(&expected), "{note}: {rendered}");
            }
        }
        (Some(true), Err(failure)) => panic!("{note}: refused as {failure}"),
        (Some(false), Ok(value)) => panic!("{note}: accepted as {value:?}"),
        (None, _) => panic!("{note}: the fixture states whether it is accepted"),
    }
}

#[test]
fn every_command_vector_lands_where_the_fixture_says_it_does() {
    for row in &rows(COMMANDS) {
        check::<FindPagesByTemplateCommand>(row);
    }
    for row in rows(COMMANDS).iter().filter(|row| row["accepted"] == Value::Bool(true)) {
        let document = text(row, "document");
        let command: FindPagesByTemplateCommand =
            serde_json::from_str(document).expect("the fixture says this is accepted");
        assert_eq!(
            serde_json::to_string(&command).expect("a command serializes"),
            document,
            "{}: rewritten differently",
            text(row, "note")
        );
    }
}

#[test]
fn a_template_matches_only_when_the_stored_value_is_that_exact_address() {
    let command: FindPagesByTemplateCommand = serde_json::from_str(
        r#"{"root_path":"/content","template_path":"/apps/example/templates/page"}"#,
    )
    .expect("a legal command");
    let vectors = rows(MATCHING);
    assert!(vectors.len() >= 11, "every near miss is covered");
    for row in &vectors {
        let stored = row["stored"].as_str();
        assert_eq!(
            command.matches_recorded(text(row, "property_type"), stored),
            row["matches"].as_bool().expect("every vector states its verdict"),
            "{}",
            text(row, "note")
        );
    }
    assert_eq!(PAGE_TEMPLATE_PROPERTY, "cq:template");
    assert_eq!(TEMPLATE_PROPERTY_TYPES, ["path", "string"], "both spellings occur in content");
}

#[test]
fn only_a_single_valued_template_property_is_read() {
    for row in &rows(CARDINALITIES) {
        assert_eq!(
            text(row, "cardinality") == "single",
            row["read"].as_bool().expect("every vector states its verdict"),
            "{}",
            text(row, "note")
        );
    }
}

#[test]
fn every_result_vector_lands_where_the_fixture_says_it_does() {
    for row in &rows(RESULTS) {
        check::<FindPagesByTemplateResult>(row);
    }
}

#[test]
fn a_result_from_another_request_is_rejected() {
    let asked: FindPagesByTemplateCommand = serde_json::from_str(
        r#"{"root_path":"/content/example","template_path":"/apps/example/templates/page"}"#,
    )
    .expect("a legal command");
    let own = FindPagesByTemplateResult::new(
        vec![PageMatch {
            repository_path: RepositoryPath::parse("/content/example/en").expect("a legal path"),
            title: None,
        }],
        None,
    )
    .expect("an ordered page");
    assert_eq!(own.require_answers(&asked), Ok(()));

    let elsewhere = FindPagesByTemplateResult::new(
        vec![PageMatch {
            repository_path: RepositoryPath::parse("/content/other").expect("a legal path"),
            title: None,
        }],
        None,
    )
    .expect("an ordered page");
    assert_eq!(elsewhere.require_answers(&asked), Err(DiscoveryResultFailure::NotThisRequest));
}
