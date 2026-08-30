//! Finding pages by component, proved exact and proved conservative.
//!
//! Resource types compare complete and exactly, with no resource-super-type
//! expansion. That is the conservative answer rather than a missing feature:
//! super-type resolution depends on the deployed application, so a discovery
//! command that guessed at it would return different results against two
//! environments holding the same content.
//!
//! `All` is satisfied across a page's subtree rather than on one resource,
//! because a page built from a header, a body, and a footer uses all three and
//! no single resource does.

use serde_json::Value;
use slingshot_domain::command::component_resource_type::ComponentResourceType;
use slingshot_domain::command::find_pages_containing_phrase::PageMatch;
use slingshot_domain::command::find_pages_using_components::{
    COMPONENT_RESOURCE_TYPE_PROPERTY, ComponentMatchMode, ComponentSearchFailure,
    FindPagesUsingComponentsCommand, FindPagesUsingComponentsResult,
    RequestedComponentResourceTypes, maximum_requested_component_resource_types,
};
use slingshot_domain::command::query_paths::DiscoveryResultFailure;
use slingshot_domain::command::repository_path::RepositoryPath;

/// Commands this test reads.
const COMMANDS: &str = include_str!("fixtures/commands/find_pages_using_components/commands.jsonl");

/// Matching vectors this test reads.
const MATCHING: &str = include_str!("fixtures/commands/find_pages_using_components/matching.jsonl");

/// Resource vectors this test reads.
const RESOURCES: &str =
    include_str!("fixtures/commands/find_pages_using_components/resources.jsonl");

/// Results this test reads.
const RESULTS: &str = include_str!("fixtures/commands/find_pages_using_components/results.jsonl");

/// Every refusal the fixtures can name, beside the sentence that produces it.
const DECLARED_REFUSALS: &[(&str, ComponentSearchFailure)] = &[
    ("TypesEmpty", ComponentSearchFailure::TypesEmpty),
    ("TypesNotUnique", ComponentSearchFailure::TypesNotUnique),
    ("TypesNotSorted", ComponentSearchFailure::TypesNotSorted),
    ("TypesTooMany", ComponentSearchFailure::TypesTooMany),
];

/// Refusals the shared discovery values make.
const DECLARED_ORDER_REFUSALS: &[(&str, DiscoveryResultFailure)] = &[
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

/// Returns every sentence a refusal can render as.
fn every_refusal_rendering() -> Vec<String> {
    DECLARED_REFUSALS
        .iter()
        .map(|(_, failure)| failure.to_string())
        .chain(DECLARED_ORDER_REFUSALS.iter().map(|(_, failure)| failure.to_string()))
        .collect()
}

/// Returns the rendering the named refusal produces.
fn refusal_rendering(reason: &str) -> Option<String> {
    if reason == CLOSED_OBJECT {
        return None;
    }
    DECLARED_REFUSALS
        .iter()
        .find(|(name, _)| *name == reason)
        .map(|(_, failure)| failure.to_string())
        .or_else(|| {
            DECLARED_ORDER_REFUSALS
                .iter()
                .find(|(name, _)| *name == reason)
                .map(|(_, failure)| failure.to_string())
        })
        .or_else(|| panic!("the fixture names a refusal this test does not know: {reason}"))
}

/// Checks one accept-or-refuse vector against the type it names.
fn check<Parsed: serde::de::DeserializeOwned + std::fmt::Debug>(row: &Value) {
    let document = text(row, "document");
    let note = text(row, "note");
    match (row["accepted"].as_bool(), serde_json::from_str::<Parsed>(document)) {
        (Some(true), Ok(_)) => (),
        (Some(false), Err(failure)) => {
            let rendered = failure.to_string();
            match refusal_rendering(text(row, "reason")) {
                Some(expected) => assert!(
                    rendered.contains(&expected),
                    "{note}: refused as {rendered}, not as {expected}"
                ),
                None => assert!(
                    !every_refusal_rendering().contains(&rendered),
                    "{note}: the closed object itself refuses this: {rendered}"
                ),
            }
        }
        (Some(true), Err(failure)) => panic!("{note}: refused as {failure}"),
        (Some(false), Ok(value)) => panic!("{note}: accepted as {value:?}"),
        (None, _) => panic!("{note}: the fixture states whether it is accepted"),
    }
}

#[test]
fn every_command_vector_lands_where_the_fixture_says_it_does() {
    let vectors = rows(COMMANDS);
    assert!(vectors.len() >= 16, "both modes, both bounds, and every refusal");
    for row in &vectors {
        check::<FindPagesUsingComponentsCommand>(row);
    }
    for row in vectors.iter().filter(|row| row["accepted"] == Value::Bool(true)) {
        let document = text(row, "document");
        let command: FindPagesUsingComponentsCommand =
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
fn one_set_of_types_has_one_spelling_on_the_wire() {
    let types = |spellings: &[&str]| {
        spellings
            .iter()
            .map(|spelling| ComponentResourceType::parse(spelling).expect("a legal type"))
            .collect::<Vec<_>>()
    };
    let unsorted = types(&["example/components/text", "example/components/header"]);
    assert_eq!(
        RequestedComponentResourceTypes::canonical(unsorted.clone()),
        Err(ComponentSearchFailure::TypesNotSorted),
        "the wire requires the canonical order rather than sorting a permutation"
    );
    let sorted = RequestedComponentResourceTypes::new(unsorted).expect("a constructor may sort");
    assert_eq!(
        sorted.types().iter().map(ComponentResourceType::as_text).collect::<Vec<_>>(),
        vec!["example/components/header", "example/components/text"],
        "and the constructor sorts once, ascending by bytes"
    );
    assert_eq!(
        RequestedComponentResourceTypes::new(Vec::new()),
        Err(ComponentSearchFailure::TypesEmpty)
    );
    let bound = usize::try_from(maximum_requested_component_resource_types())
        .expect("the bound is addressable");
    let many: Vec<ComponentResourceType> = (0..bound)
        .map(|index| {
            ComponentResourceType::parse(&format!("example/components/c{index:04}"))
                .expect("a legal type")
        })
        .collect();
    assert!(RequestedComponentResourceTypes::new(many.clone()).is_ok(), "the largest set");
    let mut over = many;
    over.push(ComponentResourceType::parse("example/components/extra").expect("a legal type"));
    assert_eq!(
        RequestedComponentResourceTypes::new(over),
        Err(ComponentSearchFailure::TypesTooMany),
        "and one further"
    );
}

#[test]
fn every_matching_vector_answers_the_way_the_fixture_says() {
    let vectors = rows(MATCHING);
    assert!(vectors.len() >= 10, "both modes, and both directions of prefix");
    for row in &vectors {
        let read = |member: &str| {
            row[member]
                .as_array()
                .expect("every vector lists its types")
                .iter()
                .map(|spelling| {
                    ComponentResourceType::parse(spelling.as_str().expect("a spelling"))
                        .expect("a legal type")
                })
                .collect::<Vec<_>>()
        };
        let wanted = read("wanted");
        let command = FindPagesUsingComponentsCommand {
            match_mode: match text(row, "match_mode") {
                "any" => ComponentMatchMode::Any,
                other => {
                    assert_eq!(other, "all", "the fixture names a mode this contract has");
                    ComponentMatchMode::All
                }
            },
            resource_types: RequestedComponentResourceTypes::new(wanted).expect("a legal set"),
            result_window: None,
            root_path: RepositoryPath::parse("/content").expect("a legal path"),
        };
        assert_eq!(
            command.matches_used(&read("used")),
            row["matches"].as_bool().expect("every vector states its verdict"),
            "{}",
            text(row, "note")
        );
    }
}

#[test]
fn only_a_single_string_resource_type_is_read() {
    assert_eq!(COMPONENT_RESOURCE_TYPE_PROPERTY, "sling:resourceType");
    for row in &rows(RESOURCES) {
        assert_eq!(
            text(row, "cardinality") == "single" && text(row, "type") == "string",
            row["read"].as_bool().expect("every vector states its verdict"),
            "{}",
            text(row, "note")
        );
    }
}

#[test]
fn every_result_vector_lands_where_the_fixture_says_it_does() {
    for row in &rows(RESULTS) {
        check::<FindPagesUsingComponentsResult>(row);
    }
}

#[test]
fn a_result_from_another_request_is_rejected() {
    let asked: FindPagesUsingComponentsCommand = serde_json::from_str(
        r#"{"match_mode":"any","resource_types":["example/components/text"],"root_path":"/content/example"}"#,
    )
    .expect("a legal command");
    let own = FindPagesUsingComponentsResult::new(
        vec![PageMatch {
            repository_path: RepositoryPath::parse("/content/example/en").expect("a legal path"),
            title: None,
        }],
        None,
    )
    .expect("an ordered page");
    assert_eq!(own.require_answers(&asked), Ok(()));

    let elsewhere = FindPagesUsingComponentsResult::new(
        vec![PageMatch {
            repository_path: RepositoryPath::parse("/content/examples").expect("a legal path"),
            title: None,
        }],
        None,
    )
    .expect("an ordered page");
    assert_eq!(elsewhere.require_answers(&asked), Err(DiscoveryResultFailure::NotThisRequest));
}
