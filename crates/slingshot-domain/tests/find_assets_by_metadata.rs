//! Finding assets by metadata, proved conservative about what it does not know.
//!
//! The claim worth proving hardest is that absence never satisfies a criterion.
//! An asset whose format could not be read is in no requested format, one whose
//! original rendition has no length is in no requested size range, and one with
//! no tags carries none of the requested ones. Treating unknown as permissible
//! would quietly widen every search that used a criterion, and the widening
//! would be invisible to the caller.
//!
//! A byte length is a JSON integer token with a stated domain, so a string, a
//! floating value, and a negative are all refused rather than coerced into
//! something plausible.

use serde_json::Value;
use slingshot_domain::command::find_assets_by_metadata::{
    ASSET_FORMAT_PROPERTY, ASSET_ORIGINAL_BINARY_PROPERTY, ASSET_PRIMARY_NODE_TYPE,
    ASSET_RENDITION_MIME_TYPE_PROPERTY, ASSET_TAGS_PROPERTY, AssetByteLength, AssetMatch,
    AssetSearchFailure, AssetTag, FindAssetsByMetadataCommand, FindAssetsByMetadataResult,
    MediaFormat, RequestedAssetTags, RequestedMediaFormats, maximum_asset_byte_length,
    maximum_requested_asset_tags, maximum_requested_media_formats,
};
use slingshot_domain::command::query_paths::DiscoveryResultFailure;
use slingshot_domain::command::repository_path::RepositoryPath;

/// Commands this test reads.
const COMMANDS: &str = include_str!("fixtures/commands/find_assets_by_metadata/commands.jsonl");

/// Ranges this test reads.
const RANGES: &str = include_str!("fixtures/commands/find_assets_by_metadata/ranges.jsonl");

/// Matching vectors this test reads.
const MATCHING: &str = include_str!("fixtures/commands/find_assets_by_metadata/matching.jsonl");

/// Format sources this test reads.
const SOURCES: &str =
    include_str!("fixtures/commands/find_assets_by_metadata/format-sources.jsonl");

/// Results this test reads.
const RESULTS: &str = include_str!("fixtures/commands/find_assets_by_metadata/results.jsonl");

/// Every refusal the fixtures can name, beside the sentence that produces it.
const DECLARED_REFUSALS: &[(&str, AssetSearchFailure)] = &[
    ("ByteLengthOutOfRange", AssetSearchFailure::ByteLengthOutOfRange),
    ("RangeInverted", AssetSearchFailure::RangeInverted),
    ("MediaFormatOutOfBounds", AssetSearchFailure::MediaFormatOutOfBounds),
    ("TagOutOfBounds", AssetSearchFailure::TagOutOfBounds),
    ("SetNotUnique", AssetSearchFailure::SetNotUnique),
    ("SetNotSorted", AssetSearchFailure::SetNotSorted),
    ("SetTooLarge", AssetSearchFailure::SetTooLarge),
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
    assert!(vectors.len() >= 25, "every criterion family and every bound");
    for row in &vectors {
        check::<FindAssetsByMetadataCommand>(row);
    }
    for row in vectors.iter().filter(|row| row["accepted"] == Value::Bool(true)) {
        let document = text(row, "document");
        let command: FindAssetsByMetadataCommand =
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
fn a_byte_length_is_an_integer_token_with_a_stated_domain() {
    assert!(AssetByteLength::new(0).is_ok(), "an empty asset has a real size");
    assert!(AssetByteLength::new(maximum_asset_byte_length()).is_ok(), "and so does the largest");
    assert_eq!(
        AssetByteLength::new(maximum_asset_byte_length() + 1),
        Err(AssetSearchFailure::ByteLengthOutOfRange),
        "one past the largest length a JCR binary reports"
    );
    for substitute in ["\"1024\"", "1.5", "-1", "1e3", "true", "null"] {
        assert!(
            serde_json::from_str::<AssetByteLength>(substitute).is_err(),
            "{substitute} is not an asset byte length"
        );
    }
    assert_eq!(
        serde_json::from_str::<AssetByteLength>("1024")
            .expect("a plain integer token is a length")
            .count(),
        1024
    );
}

#[test]
fn a_range_asks_for_something_or_is_refused() {
    for row in &rows(RANGES) {
        let build = |member: &str| {
            AssetByteLength::new(row[member].as_u64().expect("a length")).expect("in domain")
        };
        let command = FindAssetsByMetadataCommand {
            maximum_byte_length: Some(build("maximum")),
            media_formats: None,
            minimum_byte_length: Some(build("minimum")),
            property_predicates: None,
            result_window: None,
            root_path: RepositoryPath::parse("/content/dam").expect("a legal path"),
            tag_match_mode: None,
            tags: None,
        };
        let usable = row["usable"].as_bool().expect("every vector states its verdict");
        assert_eq!(command.require_usable_range().is_ok(), usable, "{}", text(row, "note"));
    }
}

#[test]
fn absence_never_satisfies_a_criterion() {
    let vectors = rows(MATCHING);
    assert!(vectors.len() >= 20, "every family, both ways, and every absence");
    for row in &vectors {
        let mut document = row["criteria"].as_object().expect("a criteria map").clone();
        document.insert("root_path".to_owned(), Value::from("/content/dam"));
        let command: FindAssetsByMetadataCommand = serde_json::from_value(Value::Object(document))
            .unwrap_or_else(|failure| panic!("{}: {failure}", text(row, "note")));
        let found: AssetMatch = serde_json::from_value(row["found"].clone())
            .unwrap_or_else(|failure| panic!("{}: {failure}", text(row, "note")));
        assert_eq!(
            command.matches_metadata(&found),
            row["matches"].as_bool().expect("every vector states its verdict"),
            "{}",
            text(row, "note")
        );
    }
}

#[test]
fn the_metadata_is_read_from_exactly_the_places_the_contract_names() {
    assert_eq!(ASSET_PRIMARY_NODE_TYPE, "dam:Asset");
    assert_eq!(ASSET_TAGS_PROPERTY, "jcr:content/metadata/cq:tags");
    assert_eq!(
        ASSET_ORIGINAL_BINARY_PROPERTY, "jcr:content/renditions/original/jcr:content/jcr:data",
        "size is the original rendition's binary, never an aggregate"
    );
    let rows = rows(SOURCES);
    let sources: Vec<&str> = rows.iter().map(|row| text(row, "property")).collect();
    assert_eq!(
        sources,
        vec![ASSET_FORMAT_PROPERTY, ASSET_RENDITION_MIME_TYPE_PROPERTY],
        "in that order, with no third fallback"
    );
}

#[test]
fn one_set_of_values_has_one_spelling_on_the_wire() {
    let formats = |spellings: &[&str]| {
        spellings
            .iter()
            .map(|spelling| MediaFormat::new(*spelling).expect("a legal format"))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        RequestedMediaFormats::canonical(formats(&["image/png", "image/jpeg"])),
        Err(AssetSearchFailure::SetNotSorted),
        "a permuted set is refused rather than sorted"
    );
    let sorted = RequestedMediaFormats::new(formats(&["image/png", "image/jpeg"]))
        .expect("a constructor may sort");
    assert_eq!(
        sorted.values().iter().map(MediaFormat::as_text).collect::<Vec<_>>(),
        vec!["image/jpeg", "image/png"]
    );
    assert_eq!(
        RequestedMediaFormats::canonical(formats(&["image/jpeg", "image/jpeg"])),
        Err(AssetSearchFailure::SetNotUnique),
        "and a repeat is reported as the repeat it is"
    );
    assert!(
        RequestedMediaFormats::new(Vec::new()).is_ok(),
        "an empty set states no format criterion, which is different from stating none"
    );

    let bound = usize::try_from(maximum_requested_asset_tags()).expect("the bound is addressable");
    let many: Vec<AssetTag> = (0..bound)
        .map(|index| AssetTag::new(format!("t{index:04}")).expect("a legal tag"))
        .collect();
    assert!(RequestedAssetTags::new(many.clone()).is_ok());
    let mut over = many;
    over.push(AssetTag::new("extra").expect("a legal tag"));
    assert_eq!(RequestedAssetTags::new(over), Err(AssetSearchFailure::SetTooLarge));
    assert!(maximum_requested_media_formats() > 0, "the format bound is a real bound");
}

#[test]
fn every_result_vector_lands_where_the_fixture_says_it_does() {
    let vectors = rows(RESULTS);
    for row in &vectors {
        check::<FindAssetsByMetadataResult>(row);
    }
    for row in vectors.iter().filter(|row| row["accepted"] == Value::Bool(true)) {
        let document = text(row, "document");
        let page: FindAssetsByMetadataResult =
            serde_json::from_str(document).expect("the fixture says this is accepted");
        assert_eq!(
            serde_json::to_string(&page).expect("a page serializes"),
            document,
            "{}: rewritten differently",
            text(row, "note")
        );
    }
}

#[test]
fn a_result_from_another_request_is_rejected() {
    let asked: FindAssetsByMetadataCommand =
        serde_json::from_str(r#"{"root_path":"/content/dam/example"}"#).expect("a legal command");
    let asset = |path: &str| AssetMatch {
        byte_length: None,
        media_format: None,
        repository_path: RepositoryPath::parse(path).expect("a legal path"),
        tags: Vec::new(),
    };
    let own = FindAssetsByMetadataResult::new(vec![asset("/content/dam/example/a.jpg")], None)
        .expect("an ordered page");
    assert_eq!(own.require_answers(&asked), Ok(()));

    let elsewhere =
        FindAssetsByMetadataResult::new(vec![asset("/content/dam/examples/a.jpg")], None)
            .expect("an ordered page");
    assert_eq!(elsewhere.require_answers(&asked), Err(DiscoveryResultFailure::NotThisRequest));
}
