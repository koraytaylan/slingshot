//! Assertions for making somewhere to put an asset.
//!
//! The target is computed from the parent and the name rather than accepted, so
//! a refusal names the address this request would have made. That is the whole
//! difference between a failure a caller can act on and one that merely repeats
//! what the caller already said.

use serde_json::Value;
use slingshot_domain::command::create_asset_folder::{
    CreateAssetFolderCommand, CreateAssetFolderFailure, CreateAssetFolderRefusal,
    CreateAssetFolderResult,
};
use slingshot_domain::command::repository_path::RepositoryPath;
use slingshot_domain::command::resource_mutation::{MutationResultFailure, ResourceMutationResult};

/// Commands this test reads.
const COMMANDS: &str = include_str!("fixtures/commands/create_asset_folder/commands.jsonl");

/// Failures this test reads.
const FAILURES: &str = include_str!("fixtures/commands/create_asset_folder/failures.jsonl");

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

/// Returns one legal request.
fn command() -> CreateAssetFolderCommand {
    serde_json::from_str(r#"{"name":"example","parent_path":"/content/dam"}"#)
        .expect("a legal command")
}

#[test]
fn every_command_vector_parses_exactly_as_the_fixture_says() {
    let vectors = rows(COMMANDS);
    assert!(vectors.len() >= 7, "every document shape and every refusal");
    for row in &vectors {
        let document = text(row, "document");
        let note = text(row, "note");
        match (
            row["accepted"].as_bool(),
            serde_json::from_str::<CreateAssetFolderCommand>(document),
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
fn the_target_is_computed_from_the_parent_and_the_name() {
    assert_eq!(command().target_path().expect("a target"), path("/content/dam/example"));
    let rooted: CreateAssetFolderCommand =
        serde_json::from_str(r#"{"name":"example","parent_path":"/"}"#).expect("a legal command");
    assert_eq!(rooted.target_path().expect("a target"), path("/example"));
}

#[test]
fn a_result_answers_only_the_request_that_computed_its_address() {
    let answered = CreateAssetFolderResult {
        mutated: ResourceMutationResult { repository_path: path("/content/dam/example") },
    };
    assert_eq!(answered.require_answers(&command()), Ok(()));
    let elsewhere = CreateAssetFolderResult {
        mutated: ResourceMutationResult { repository_path: path("/content/dam/other") },
    };
    assert_eq!(elsewhere.require_answers(&command()), Err(MutationResultFailure::NotThisRequest));
}

#[test]
fn a_refusal_answers_only_the_request_that_computed_its_target() {
    let refusal = CreateAssetFolderRefusal {
        failure: CreateAssetFolderFailure::TargetAlreadyExists,
        target_path: path("/content/dam/example"),
    };
    assert_eq!(refusal.require_answers(&command()), Ok(()));
    let elsewhere = CreateAssetFolderRefusal {
        failure: CreateAssetFolderFailure::TargetAlreadyExists,
        target_path: path("/content/dam/other"),
    };
    assert_eq!(elsewhere.require_answers(&command()), Err(MutationResultFailure::NotThisRequest));
}

#[test]
fn every_failure_document_carries_its_members_and_says_what_it_proves() {
    let vectors = rows(FAILURES);
    assert_eq!(vectors.len(), 6, "one for each category this command allows");
    for row in &vectors {
        let note = text(row, "note");
        let document = text(row, "document");
        let refusal: CreateAssetFolderRefusal =
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
        serde_json::from_str::<CreateAssetFolderRefusal>(
            r#"{"failure":"parent_not_found","target_path":"/content/dam/example","extra":1}"#
        )
        .is_err(),
        "a surplus member was accepted"
    );
}
