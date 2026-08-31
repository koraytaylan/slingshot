//! Assertions for putting a binary into the repository.
//!
//! Two things are pinned here that nothing else in the registry needs. The
//! payload is bounded twice and the encoded bound is checked first, which the
//! shared payload assertions prove; and the reported rendition length has to be
//! the decoded payload's own length, because that is the single fact about the
//! bytes that the request determines and an author therefore cannot restate.

use serde_json::Value;
use slingshot_domain::command::create_asset::{
    CreateAssetCommand, CreateAssetFailure, CreateAssetRefusal, CreateAssetResult,
};
use slingshot_domain::command::find_assets_by_metadata::AssetByteLength;
use slingshot_domain::command::repository_path::RepositoryPath;
use slingshot_domain::command::resource_mutation::MutationResultFailure;

/// Commands this test reads.
const COMMANDS: &str = include_str!("fixtures/commands/create_asset/commands.jsonl");

/// Failures this test reads.
const FAILURES: &str = include_str!("fixtures/commands/create_asset/failures.jsonl");

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

/// Returns one legal request carrying five decoded bytes.
fn command() -> CreateAssetCommand {
    serde_json::from_str(
        r#"{"name":"logo.png","parent_path":"/content/dam/example","payload":{"media_type":"image/png","encoded_content":"aGVsbG8="}}"#,
    )
    .expect("a legal command")
}

/// Returns one length value.
fn length(value: u64) -> AssetByteLength {
    AssetByteLength::new(value).expect("a legal length")
}

#[test]
fn every_command_vector_parses_exactly_as_the_fixture_says() {
    let vectors = rows(COMMANDS);
    assert!(vectors.len() >= 6, "every document shape and every refusal");
    for row in &vectors {
        let document = text(row, "document");
        let note = text(row, "note");
        match (row["accepted"].as_bool(), serde_json::from_str::<CreateAssetCommand>(document)) {
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
fn the_payload_reports_the_length_it_decodes_to() {
    let asked = command();
    assert_eq!(asked.payload_byte_length(), u64::try_from(b"hello".len()).expect("it fits"));
    assert_eq!(asked.target_path().expect("a target"), path("/content/dam/example/logo.png"));
}

#[test]
fn a_result_reporting_another_length_than_the_payload_decodes_to_is_refused() {
    let asked = command();
    let answered = CreateAssetResult {
        original_rendition_byte_length: length(asked.payload_byte_length()),
        repository_path: path("/content/dam/example/logo.png"),
    };
    assert_eq!(answered.require_answers(&asked), Ok(()));
    let longer = CreateAssetResult {
        original_rendition_byte_length: length(asked.payload_byte_length() + 1),
        repository_path: path("/content/dam/example/logo.png"),
    };
    assert_eq!(longer.require_answers(&asked), Err(MutationResultFailure::NotThisRequest));
}

#[test]
fn a_result_answers_only_the_request_that_computed_its_address() {
    let asked = command();
    let elsewhere = CreateAssetResult {
        original_rendition_byte_length: length(asked.payload_byte_length()),
        repository_path: path("/content/dam/example/other.png"),
    };
    assert_eq!(elsewhere.require_answers(&asked), Err(MutationResultFailure::NotThisRequest));
}

#[test]
fn a_refusal_answers_only_the_request_that_computed_its_target() {
    let refusal = CreateAssetRefusal {
        failure: CreateAssetFailure::PayloadTooLarge,
        target_path: path("/content/dam/example/logo.png"),
    };
    assert_eq!(refusal.require_answers(&command()), Ok(()));
    let elsewhere = CreateAssetRefusal {
        failure: CreateAssetFailure::PayloadTooLarge,
        target_path: path("/content/dam/example/other.png"),
    };
    assert_eq!(elsewhere.require_answers(&command()), Err(MutationResultFailure::NotThisRequest));
}

#[test]
fn every_failure_document_carries_its_members_and_says_what_it_proves() {
    let vectors = rows(FAILURES);
    assert_eq!(vectors.len(), 8, "one for each category this command allows");
    for row in &vectors {
        let note = text(row, "note");
        let document = text(row, "document");
        let refusal: CreateAssetRefusal =
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
        serde_json::from_str::<CreateAssetRefusal>(r#"{"failure":"payload_rejected","target_path":"/content/dam/example/logo.png","extra":1}"#).is_err(),
        "a surplus member was accepted"
    );
}
