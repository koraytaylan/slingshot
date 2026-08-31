//! Assertions for writing the metadata that asset searches read.
//!
//! The metadata resource is two children below the asset, and this command
//! computes that address rather than accepting one. A caller that could name it
//! could write asset metadata onto something that is not an asset's metadata,
//! and the search that reads it would never say so.

use serde_json::Value;
use slingshot_domain::command::repository_path::RepositoryPath;
use slingshot_domain::command::resource_mutation::{
    MutationResultFailure, PropertyMutationFailure, ResourceMutationResult,
};
use slingshot_domain::command::update_asset_metadata::{
    UpdateAssetMetadataCommand, UpdateAssetMetadataFailure, UpdateAssetMetadataRefusal,
    UpdateAssetMetadataResult,
};

/// Commands this test reads.
const COMMANDS: &str = include_str!("fixtures/commands/update_asset_metadata/commands.jsonl");

/// Parsable requests the shared mutation rule then refuses.
const UNUSABLE: &str = include_str!("fixtures/commands/update_asset_metadata/unusable.jsonl");

/// Failures this test reads.
const FAILURES: &str = include_str!("fixtures/commands/update_asset_metadata/failures.jsonl");

/// Metadata resource the asset in these fixtures keeps its metadata at.
const METADATA: &str = "/content/dam/example/logo.png/jcr:content/metadata";

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
fn command() -> UpdateAssetMetadataCommand {
    serde_json::from_str(
        r#"{"asset_path":"/content/dam/example/logo.png","removed_property_names":["dc:title"]}"#,
    )
    .expect("a legal command")
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
            serde_json::from_str::<UpdateAssetMetadataCommand>(document),
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
fn the_metadata_resource_is_computed_from_the_asset_rather_than_accepted() {
    assert_eq!(command().metadata_path().expect("a metadata resource"), path(METADATA));
}

#[test]
fn the_shared_mutation_rule_applies_here_unchanged() {
    for row in rows(UNUSABLE) {
        let note = text(&row, "note");
        let parsed: UpdateAssetMetadataCommand = serde_json::from_str(text(&row, "document"))
            .unwrap_or_else(|failure| panic!("{note}: the document did not parse: {failure}"));
        let expected = match text(&row, "refusal") {
            "changes_nothing" => PropertyMutationFailure::ChangesNothing,
            "both_assigned_and_removed" => PropertyMutationFailure::BothAssignedAndRemoved,
            other => panic!("{note}: the fixture names an unknown refusal {other}"),
        };
        assert_eq!(parsed.require_usable(), Err(expected), "{note}");
    }
}

#[test]
fn a_result_answers_only_the_request_that_computed_its_address() {
    let answered = UpdateAssetMetadataResult {
        mutated: ResourceMutationResult { repository_path: path(METADATA) },
    };
    assert_eq!(answered.require_answers(&command()), Ok(()));
    let elsewhere = UpdateAssetMetadataResult {
        mutated: ResourceMutationResult {
            repository_path: path("/content/dam/example/other.png/jcr:content/metadata"),
        },
    };
    assert_eq!(elsewhere.require_answers(&command()), Err(MutationResultFailure::NotThisRequest));
}

#[test]
fn a_refusal_answers_only_the_request_that_named_its_asset() {
    let refusal = UpdateAssetMetadataRefusal {
        asset_path: path("/content/dam/example/logo.png"),
        failure: UpdateAssetMetadataFailure::AssetNotFound,
    };
    assert_eq!(refusal.require_answers(&command()), Ok(()));
    let elsewhere = UpdateAssetMetadataRefusal {
        asset_path: path("/content/dam/example/other.png"),
        failure: UpdateAssetMetadataFailure::AssetNotFound,
    };
    assert_eq!(elsewhere.require_answers(&command()), Err(MutationResultFailure::NotThisRequest));
}

#[test]
fn every_failure_document_carries_its_members_and_says_what_it_proves() {
    let vectors = rows(FAILURES);
    assert_eq!(vectors.len(), 7, "one for each category this command allows");
    for row in &vectors {
        let note = text(row, "note");
        let document = text(row, "document");
        let refusal: UpdateAssetMetadataRefusal =
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
        serde_json::from_str::<UpdateAssetMetadataRefusal>(r#"{"asset_path":"/content/dam/example/logo.png","failure":"asset_not_found","extra":1}"#).is_err(),
        "a surplus member was accepted"
    );
}
