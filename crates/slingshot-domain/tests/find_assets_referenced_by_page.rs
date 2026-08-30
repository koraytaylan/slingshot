//! Finding a page's asset references, proved to report only real ones.
//!
//! A reference survives three checks: the property type is one a reference is
//! stored in, the value validates as an absolute repository path, and the node
//! it addresses is an asset. Prose that mentions a path, a relative spelling, a
//! path that addresses nothing, and one that addresses a page all fail at least
//! one of them, and none is reported.
//!
//! That is the point rather than caution for its own sake. Reporting
//! path-shaped strings would make every page look as though it referred to
//! whatever its authors happened to type.

use serde_json::Value;
use slingshot_domain::command::find_assets_referenced_by_page::{
    FindAssetsReferencedByPageCommand, FindAssetsReferencedByPageResult, PageAnchorRefusal,
    REFERENCE_PROPERTY_TYPES, ReferenceSearchFailure, ReferencedAssetMatch,
    maximum_asset_reference_paths,
};
use slingshot_domain::command::query_paths::DiscoveryResultFailure;
use slingshot_domain::command::repository_path::{RelativePropertyPath, RepositoryPath};

/// Commands this test reads.
const COMMANDS: &str =
    include_str!("fixtures/commands/find_assets_referenced_by_page/commands.jsonl");

/// Reference vectors this test reads.
const REFERENCES: &str =
    include_str!("fixtures/commands/find_assets_referenced_by_page/references.jsonl");

/// Match vectors this test reads.
const MATCHES: &str =
    include_str!("fixtures/commands/find_assets_referenced_by_page/matches.jsonl");

/// Anchor failures this test reads.
const ANCHORS: &str =
    include_str!("fixtures/commands/find_assets_referenced_by_page/anchors.jsonl");

/// Results this test reads.
const RESULTS: &str =
    include_str!("fixtures/commands/find_assets_referenced_by_page/results.jsonl");

/// Every refusal the fixtures can name, beside the sentence that produces it.
const DECLARED_REFUSALS: &[(&str, ReferenceSearchFailure)] = &[
    ("ReferencePathsEmpty", ReferenceSearchFailure::ReferencePathsEmpty),
    (
        "ReferencePathsNotStrictlyAscending",
        ReferenceSearchFailure::ReferencePathsNotStrictlyAscending,
    ),
    ("ReferencePathsTooMany", ReferenceSearchFailure::ReferencePathsTooMany),
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
    for row in &rows(COMMANDS) {
        check::<FindAssetsReferencedByPageCommand>(row);
    }
    for row in rows(COMMANDS).iter().filter(|row| row["accepted"] == Value::Bool(true)) {
        let document = text(row, "document");
        let command: FindAssetsReferencedByPageCommand =
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
fn only_a_value_that_survives_all_three_checks_is_a_reference() {
    let vectors = rows(REFERENCES);
    assert!(vectors.len() >= 12, "each check fails at least once on its own");
    for row in &vectors {
        assert_eq!(
            FindAssetsReferencedByPageCommand::is_reference(
                text(row, "property_type"),
                text(row, "stored"),
                row["resolves_to_asset"].as_bool().expect("every vector says what it resolved to"),
            ),
            row["is_reference"].as_bool().expect("every vector states its verdict"),
            "{}",
            text(row, "note")
        );
    }
    assert_eq!(REFERENCE_PROPERTY_TYPES, ["path", "string"], "both spellings occur in content");
}

#[test]
fn a_reference_that_resolves_to_nothing_is_reported_as_nothing() {
    let missing = "/content/dam/example/missing.jpg";
    assert!(
        RepositoryPath::parse(missing).is_ok(),
        "the value is a perfectly good repository path"
    );
    assert!(
        !FindAssetsReferencedByPageCommand::is_reference("string", missing, false),
        "and it is still not a reference, because nothing is there"
    );
    assert!(
        FindAssetsReferencedByPageCommand::is_reference("string", missing, true),
        "the same value is a reference exactly when an asset is there"
    );
}

#[test]
fn every_match_vector_lands_where_the_fixture_says_it_does() {
    let vectors = rows(MATCHES);
    for row in &vectors {
        check::<ReferencedAssetMatch>(row);
    }
    for row in vectors.iter().filter(|row| row["accepted"] == Value::Bool(true)) {
        let document = text(row, "document");
        let found: ReferencedAssetMatch =
            serde_json::from_str(document).expect("the fixture says this is accepted");
        assert_eq!(
            serde_json::to_string(&found).expect("a match serializes"),
            document,
            "{}: rewritten differently",
            text(row, "note")
        );
    }
}

#[test]
fn one_place_is_recorded_once() {
    let asset = RepositoryPath::parse("/content/dam/a.jpg").expect("a legal path");
    let place = |spelling: &str| RelativePropertyPath::parse(spelling).expect("a legal path");
    assert_eq!(
        ReferencedAssetMatch::new(asset.clone(), Vec::new()),
        Err(ReferenceSearchFailure::ReferencePathsEmpty),
        "a match with nowhere it was found is not a match"
    );
    assert_eq!(
        ReferencedAssetMatch::new(
            asset.clone(),
            vec![place("par/image/fileReference"), place("par/image/fileReference")]
        ),
        Err(ReferenceSearchFailure::ReferencePathsNotStrictlyAscending)
    );
    let bound = usize::try_from(maximum_asset_reference_paths()).expect("addressable");
    let many: Vec<RelativePropertyPath> =
        (0..bound).map(|index| place(&format!("par/p{index:04}/fileReference"))).collect();
    assert!(ReferencedAssetMatch::new(asset.clone(), many.clone()).is_ok());
    let mut over = many;
    over.push(place("par/extra/fileReference"));
    assert_eq!(
        ReferencedAssetMatch::new(asset, over),
        Err(ReferenceSearchFailure::ReferencePathsTooMany)
    );
}

#[test]
fn the_three_anchor_failures_stay_three_different_answers() {
    let anchors = rows(ANCHORS);
    let declared: Vec<&Value> =
        anchors.iter().filter(|row| row["refused"] != Value::Bool(true)).collect();
    assert_eq!(declared.len(), 3, "not found, access denied, and not a page");
    for row in &declared {
        let document = text(row, "document");
        let refusal: PageAnchorRefusal =
            serde_json::from_str(document).expect("every anchor vector is a legal failure");
        assert_eq!(
            serde_json::to_string(&refusal).expect("a failure serializes"),
            document,
            "{}: rewritten differently",
            text(row, "note")
        );
        let members: Vec<String> = serde_json::from_str::<Value>(document)
            .expect("one object")
            .as_object()
            .expect("an object")
            .keys()
            .cloned()
            .collect();
        assert_eq!(members, vec!["failure".to_owned(), "page_path".to_owned()]);
    }
    for row in rows(ANCHORS).iter().filter(|row| row["refused"] == Value::Bool(true)) {
        assert!(
            serde_json::from_str::<PageAnchorRefusal>(text(row, "document")).is_err(),
            "{}: accepted",
            text(row, "note")
        );
    }
}

#[test]
fn every_result_vector_lands_where_the_fixture_says_it_does() {
    for row in &rows(RESULTS) {
        check::<FindAssetsReferencedByPageResult>(row);
    }
}

#[test]
fn a_refusal_or_a_result_from_another_request_is_rejected() {
    let asked: FindAssetsReferencedByPageCommand =
        serde_json::from_str(r#"{"page_path":"/content/example/en"}"#).expect("a legal command");
    let elsewhere: PageAnchorRefusal =
        serde_json::from_str(r#"{"failure":"page_invalid","page_path":"/content/other"}"#)
            .expect("a legal refusal");
    assert_eq!(
        elsewhere.require_answers(&asked),
        Err(DiscoveryResultFailure::NotThisRequest),
        "the echoed page is the only thing telling two refusals apart"
    );
    let own: PageAnchorRefusal =
        serde_json::from_str(r#"{"failure":"page_invalid","page_path":"/content/example/en"}"#)
            .expect("a legal refusal");
    assert_eq!(own.require_answers(&asked), Ok(()));

    let itself = FindAssetsReferencedByPageResult::new(
        vec![
            ReferencedAssetMatch::new(
                RepositoryPath::parse("/content/example/en").expect("a legal path"),
                vec![RelativePropertyPath::parse("par/fileReference").expect("a legal path")],
            )
            .expect("a legal match"),
        ],
        None,
    )
    .expect("an ordered page");
    assert_eq!(
        itself.require_answers(&asked),
        Err(DiscoveryResultFailure::NotThisRequest),
        "a page does not refer to itself, and a result saying so is not this request's"
    );
}
