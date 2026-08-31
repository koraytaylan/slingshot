//! Assertions for resolving an address and mapping a resource.
//!
//! Two rules carry both commands. Not resolving is an answer rather than a
//! failure, because an address that reaches nothing is exactly what a
//! misconfiguration looks like and the caller asked to be told; and a trace is
//! present exactly when the request asked for one, so the size of an answer
//! depends on the request rather than on the deployment.

use serde_json::Value;
use slingshot_domain::command::command_identity::CommandContract;
use slingshot_domain::command::repository_path::RepositoryPath;
use slingshot_domain::command::resource_mapping_entry::{RequestAddress, ResourceMappingFailure};
use slingshot_domain::command::resource_resolution::{
    MapResourcePathCommand, MapResourcePathResult, ResolveResourcePathCommand,
    ResolveResourcePathResult,
};

/// Commands this test reads.
const COMMANDS: &str = include_str!("fixtures/commands/resource_resolution/commands.jsonl");

/// Mapping requests this test reads.
const MAPPING: &str = include_str!("fixtures/commands/resource_resolution/mapping.jsonl");

/// The address every resolution vector asks about.
const ADDRESS: &str = "/en/report.html";

/// The resource every mapping vector asks about.
const RESOURCE: &str = "/content/example/en/report";

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

/// Returns one legal request address.
fn address(value: &str) -> RequestAddress {
    RequestAddress::parse(value).expect("a legal request address")
}

/// Returns one resolution request.
fn resolve(include_trace: bool) -> ResolveResourcePathCommand {
    ResolveResourcePathCommand { include_trace, request_address: address(ADDRESS) }
}

/// Returns one resolution result.
fn resolved(resolved_path: Option<&str>, trace: Option<Vec<&str>>) -> ResolveResourcePathResult {
    ResolveResourcePathResult {
        extension: Some("html".to_owned()),
        request_address: address(ADDRESS),
        resolved_path: resolved_path.map(path),
        resource_type: None,
        selectors: Vec::new(),
        suffix: None,
        trace: trace.map(|entries| entries.into_iter().map(path).collect()),
    }
}

#[test]
fn every_command_vector_parses_exactly_as_the_fixture_says() {
    let vectors = rows(COMMANDS);
    assert!(vectors.len() >= 5, "every document shape and every refusal");
    for row in &vectors {
        let document = text(row, "document");
        let note = text(row, "note");
        match (
            row["accepted"].as_bool(),
            serde_json::from_str::<ResolveResourcePathCommand>(document),
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
fn every_mapping_vector_parses_exactly_as_the_fixture_says() {
    let vectors = rows(MAPPING);
    assert!(vectors.len() >= 4, "both authorities and every refusal");
    for row in &vectors {
        let document = text(row, "document");
        let note = text(row, "note");
        match (row["accepted"].as_bool(), serde_json::from_str::<MapResourcePathCommand>(document))
        {
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
fn an_address_that_reaches_nothing_is_an_answer_rather_than_a_failure() {
    let nothing = resolved(None, None);
    assert_eq!(nothing.require_answers(&resolve(false)), Ok(()));
    let written = serde_json::to_string(&nothing).expect("a result serializes");
    assert!(!written.contains("resolved_path"), "an unresolved path was serialized");
}

#[test]
fn a_trace_is_present_exactly_when_the_request_asked_for_one() {
    assert_eq!(
        resolved(Some("/content/example/en/report"), Some(vec!["/etc/map/https/example.test"]))
            .require_answers(&resolve(true)),
        Ok(())
    );
    assert_eq!(
        resolved(Some("/content/example/en/report"), Some(vec!["/etc/map/https/example.test"]))
            .require_answers(&resolve(false)),
        Err(ResourceMappingFailure::TraceMisplaced),
        "a trace nobody asked for was accepted"
    );
    assert_eq!(
        resolved(Some("/content/example/en/report"), None).require_answers(&resolve(true)),
        Err(ResourceMappingFailure::TraceMisplaced),
        "a resolution that found something reported no trace where one was asked for"
    );
    assert_eq!(
        resolved(None, None).require_answers(&resolve(true)),
        Ok(()),
        "a resolution that found nothing had no entries to report"
    );
}

#[test]
fn a_trace_is_accepted_at_its_bound_and_refused_one_entry_past_it() {
    let bound =
        usize::try_from(CommandContract::embedded().limit("maximum_resolution_trace_entries"))
            .expect("the bound fits");
    let entries: Vec<String> = (0..=bound).map(|index| format!("/etc/map/e{index}")).collect();
    let exact: Vec<&str> = entries[..bound].iter().map(String::as_str).collect();
    assert_eq!(
        resolved(Some("/content/example/en/report"), Some(exact)).require_answers(&resolve(true)),
        Ok(())
    );
    let beyond: Vec<&str> = entries.iter().map(String::as_str).collect();
    assert_eq!(
        resolved(Some("/content/example/en/report"), Some(beyond)).require_answers(&resolve(true)),
        Err(ResourceMappingFailure::TraceTooLong)
    );
}

#[test]
fn each_result_answers_only_the_request_that_named_its_subject() {
    let elsewhere = ResolveResourcePathResult {
        extension: None,
        request_address: address("/en/other.html"),
        resolved_path: None,
        resource_type: None,
        selectors: Vec::new(),
        suffix: None,
        trace: None,
    };
    assert_eq!(
        elsewhere.require_answers(&resolve(false)),
        Err(ResourceMappingFailure::NotThisRequest)
    );

    let asked = MapResourcePathCommand {
        include_trace: false,
        request_authority: None,
        repository_path: path(RESOURCE),
    };
    let mapped = MapResourcePathResult {
        mapped_address: address("https://example.test/en/report.html"),
        repository_path: path(RESOURCE),
        trace: None,
    };
    assert_eq!(mapped.require_answers(&asked), Ok(()));
    let other = MapResourcePathResult {
        mapped_address: address("https://example.test/en/report.html"),
        repository_path: path("/content/example/en/other"),
        trace: None,
    };
    assert_eq!(other.require_answers(&asked), Err(ResourceMappingFailure::NotThisRequest));
}
