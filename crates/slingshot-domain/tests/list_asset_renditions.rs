//! Assertions for listing what an asset actually offers a consumer.
//!
//! Two rules carry this command. Rows are ordered by rendition name, because
//! that is the name a consumer asks for and the one a caller resumes from; and
//! every address reported has to be under the asset the request named, because a
//! rendition of something else is not this asset's rendition however plausible
//! its name looks.

use serde_json::Value;
use slingshot_domain::command::command_identity::CommandContract;
use slingshot_domain::command::list_asset_renditions::{
    ListAssetRenditionsCommand, ListAssetRenditionsResult, RenditionName,
};
use slingshot_domain::command::operational_listing::ListingResultFailure;
use slingshot_domain::command::repository_path::RepositoryPath;
use slingshot_domain::command::result_window::ResultWindow;

/// Commands this test reads.
const COMMANDS: &str = include_str!("fixtures/commands/list_asset_renditions/commands.jsonl");

/// Asset every vector addresses.
const ASSET: &str = "/content/dam/example/logo.png";

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

/// Returns one rendition row.
fn rendition(name: &str, under: &str) -> Value {
    serde_json::json!({
        "byte_length": 1024,
        "media_type": "image/png",
        "name": name,
        "repository_path": format!("{under}/jcr:content/renditions/{name}"),
    })
}

/// Returns one result document carrying `matches`.
fn result(matches: Vec<Value>) -> String {
    serde_json::to_string(&serde_json::json!({"matches": matches})).expect("a document")
}

/// Returns one legal request.
fn command() -> ListAssetRenditionsCommand {
    ListAssetRenditionsCommand { asset_path: path(ASSET), result_window: None }
}

#[test]
fn every_command_vector_parses_exactly_as_the_fixture_says() {
    let vectors = rows(COMMANDS);
    assert!(vectors.len() >= 5, "both window forms and every refusal");
    for row in &vectors {
        let document = text(row, "document");
        let note = text(row, "note");
        match (
            row["accepted"].as_bool(),
            serde_json::from_str::<ListAssetRenditionsCommand>(document),
        ) {
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
fn an_absent_window_resolves_to_the_default_one() {
    assert_eq!(command().resolved_window(), ResultWindow::default());
}

#[test]
fn an_empty_page_and_a_strictly_ascending_page_both_round_trip() {
    for matches in [
        Vec::new(),
        vec![rendition("original", ASSET)],
        vec![rendition("cq5dam.thumbnail.png", ASSET), rendition("original", ASSET)],
    ] {
        let document = result(matches);
        let parsed: ListAssetRenditionsResult =
            serde_json::from_str(&document).expect("a legal result");
        assert_eq!(serde_json::to_string(&parsed).expect("a result serializes"), document);
    }
}

#[test]
fn a_repeated_or_descending_rendition_name_is_refused() {
    for matches in [
        vec![rendition("original", ASSET), rendition("original", ASSET)],
        vec![rendition("original", ASSET), rendition("cq5dam.thumbnail.png", ASSET)],
    ] {
        assert!(
            serde_json::from_str::<ListAssetRenditionsResult>(&result(matches)).is_err(),
            "an unordered page was accepted"
        );
    }
}

#[test]
fn a_rendition_addressed_outside_the_asset_is_refused_by_request_context() {
    let inside: ListAssetRenditionsResult =
        serde_json::from_str(&result(vec![rendition("original", ASSET)])).expect("a legal result");
    assert_eq!(inside.require_answers(&command()), Ok(()));
    let outside: ListAssetRenditionsResult = serde_json::from_str(&result(vec![rendition(
        "original",
        "/content/dam/example/other.png",
    )]))
    .expect("a legal result");
    assert_eq!(outside.require_answers(&command()), Err(ListingResultFailure::NotThisRequest));
}

#[test]
fn a_rendition_name_is_accepted_at_its_bound_and_refused_one_byte_past_it() {
    let bound = usize::try_from(CommandContract::embedded().limit("maximum_rendition_name_bytes"))
        .expect("the bound fits");
    let exact = "a".repeat(bound);
    assert!(RenditionName::parse(&exact).is_ok(), "the bound itself was refused");
    assert!(
        RenditionName::parse(&format!("{exact}a")).is_err(),
        "one byte past the bound was accepted"
    );
    for refused in ["", " original", "original ", "renditions/original", "orig\u{0}inal"] {
        assert!(RenditionName::parse(refused).is_err(), "{refused:?} was accepted");
    }
}
