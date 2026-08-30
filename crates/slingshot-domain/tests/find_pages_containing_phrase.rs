//! Finding pages by phrase, proved literal.
//!
//! Everything here is about not being clever. The phrase matches one contiguous
//! run of Unicode scalar values with no normalization, no case folding, no
//! stemming, and no tokenization, so a caller who searches for one spelling
//! gets that spelling and learns what is actually stored. A composed `café` does
//! not find its decomposed twin, and the fixture holds both so the difference is
//! a vector rather than a claim.
//!
//! The phrase is also never trimmed. Leading or trailing whitespace is refused
//! as a second spelling of one phrase rather than removed, and the fixture walks
//! several Unicode whitespace scalars a naive ASCII check would let through.

use serde_json::Value;
use slingshot_domain::command::find_pages_containing_phrase::{
    FindPagesContainingPhraseCommand, FindPagesContainingPhraseResult, PAGE_CONTENT_CHILD,
    PAGE_PRIMARY_NODE_TYPE, PAGE_TITLE_PROPERTY, PageMatch, PageSearchFailure, PageTitle,
    SearchPhrase, maximum_page_title_bytes, maximum_search_phrase_bytes,
};
use slingshot_domain::command::query_paths::{AnchorRefusal, DiscoveryResultFailure};
use slingshot_domain::command::repository_path::RepositoryPath;

/// Phrases this test reads.
const PHRASES: &str = include_str!("fixtures/commands/find_pages_containing_phrase/phrases.jsonl");

/// Commands this test reads.
const COMMANDS: &str =
    include_str!("fixtures/commands/find_pages_containing_phrase/commands.jsonl");

/// Results this test reads.
const RESULTS: &str = include_str!("fixtures/commands/find_pages_containing_phrase/results.jsonl");

/// Scenarios this test reads.
const SCENARIOS: &str =
    include_str!("fixtures/commands/find_pages_containing_phrase/scenarios.jsonl");

/// Anchor failures this test reads.
const ANCHORS: &str = include_str!("fixtures/commands/find_pages_containing_phrase/anchors.jsonl");

/// Every refusal the fixtures can name, beside the variant that produces it.
const DECLARED_REFUSALS: &[(&str, PageSearchFailure)] = &[
    ("PhraseOutOfBounds", PageSearchFailure::PhraseOutOfBounds),
    ("PhraseNotCanonical", PageSearchFailure::PhraseNotCanonical),
    ("PhraseControlCharacter", PageSearchFailure::PhraseControlCharacter),
    ("TitleTooLong", PageSearchFailure::TitleTooLong),
];

/// Refusals the shared discovery values make, which the results fixture names.
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

/// Returns every sentence a page-search refusal can render as.
fn every_refusal_rendering() -> Vec<String> {
    DECLARED_REFUSALS
        .iter()
        .map(|(_, failure)| failure.to_string())
        .chain(DECLARED_ORDER_REFUSALS.iter().map(|(_, failure)| failure.to_string()))
        .collect()
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
fn every_phrase_vector_lands_where_the_fixture_says_it_does() {
    let vectors = rows(PHRASES);
    assert!(vectors.len() >= 19, "several Unicode whitespace scalars, not just the space");
    for row in &vectors {
        let spelling = text(row, "spelling");
        let note = text(row, "note");
        match (row["accepted"].as_bool(), SearchPhrase::new(spelling)) {
            (Some(true), Ok(phrase)) => assert_eq!(
                phrase.as_text(),
                spelling,
                "{note}: the phrase was rewritten rather than preserved"
            ),
            (Some(false), Err(failure)) => assert_eq!(
                Some(failure.to_string()),
                refusal_rendering(text(row, "reason")),
                "{note}"
            ),
            (_, built) => panic!("{note}: the phrase answered {built:?}"),
        }
    }
}

#[test]
fn two_spellings_of_one_word_stay_two_phrases() {
    let composed = SearchPhrase::new("caf\u{e9}").expect("a legal phrase");
    let decomposed = SearchPhrase::new("cafe\u{301}").expect("a legal phrase");
    assert_ne!(composed, decomposed, "nothing normalized either one");
    assert_eq!(composed.as_text().len(), 5, "three ASCII bytes and one two-byte scalar");
    assert_eq!(decomposed.as_text().len(), 6, "and the bytes are the ones that arrived");
    assert!(!composed.occurs_in(decomposed.as_text()), "neither finds the other");
    assert!(!decomposed.occurs_in(composed.as_text()));
}

#[test]
fn every_occurrence_vector_answers_the_way_the_fixture_says() {
    let vectors: Vec<Value> =
        rows(SCENARIOS).into_iter().filter(|row| text(row, "kind") == "occurrence").collect();
    assert!(vectors.len() >= 12, "case, normalization, stemming, and adjacency are covered");
    for row in &vectors {
        let phrase = SearchPhrase::new(text(row, "phrase")).expect("a legal phrase");
        assert_eq!(
            phrase.occurs_in(text(row, "stored")),
            row["found"].as_bool().expect("every vector states its answer"),
            "{}",
            text(row, "note")
        );
    }
}

#[test]
fn every_command_vector_lands_where_the_fixture_says_it_does() {
    for row in &rows(COMMANDS) {
        check::<FindPagesContainingPhraseCommand>(row);
    }
}

#[test]
fn every_accepted_command_writes_itself_back_byte_for_byte() {
    for row in rows(COMMANDS).iter().filter(|row| row["accepted"] == Value::Bool(true)) {
        let document = text(row, "document");
        let command: FindPagesContainingPhraseCommand =
            serde_json::from_str(document).expect("the fixture says this is accepted");
        assert_eq!(
            serde_json::to_string(&command).expect("a valid command serializes"),
            document,
            "{}: rewritten differently",
            text(row, "note")
        );
    }
}

#[test]
fn every_result_vector_lands_where_the_fixture_says_it_does() {
    for row in &rows(RESULTS) {
        check::<FindPagesContainingPhraseResult>(row);
    }
    for row in rows(RESULTS).iter().filter(|row| row["accepted"] == Value::Bool(true)) {
        let document = text(row, "document");
        let page: FindPagesContainingPhraseResult =
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
fn a_title_comes_only_from_a_single_string_directly_on_the_content_resource() {
    assert_eq!(PAGE_PRIMARY_NODE_TYPE, "cq:Page");
    assert_eq!(PAGE_CONTENT_CHILD, "jcr:content");
    assert_eq!(PAGE_TITLE_PROPERTY, "jcr:title");
    for row in rows(SCENARIOS).iter().filter(|row| text(row, "kind") == "title") {
        let note = text(row, "note");
        let reported = row["reported"].as_bool().expect("every vector states its verdict");
        let expected = row["property"].as_object().is_some_and(|property| {
            property["cardinality"] == "single" && property["type"] == "string"
        });
        assert_eq!(expected, reported, "{note}");
    }
    let bound = usize::try_from(maximum_page_title_bytes()).expect("the bound is addressable");
    assert!(PageTitle::new("t".repeat(bound)).is_ok(), "the longest title is a title");
    assert_eq!(
        PageTitle::new("t".repeat(bound + 1)),
        Err(PageSearchFailure::TitleTooLong),
        "and one byte further is not"
    );
    assert!(PageTitle::new("").is_ok(), "an empty title is stored content, not an absence");
}

#[test]
fn only_an_exact_page_is_a_candidate() {
    for row in rows(SCENARIOS).iter().filter(|row| text(row, "kind") == "candidate") {
        assert_eq!(
            text(row, "primary_node_type") == PAGE_PRIMARY_NODE_TYPE,
            row["candidate"].as_bool().expect("every vector states its verdict"),
            "{}",
            text(row, "note")
        );
    }
}

#[test]
fn an_anchor_failure_carries_its_root_and_proves_nothing_was_enumerated() {
    for row in &rows(ANCHORS) {
        let document = text(row, "document");
        let refusal: AnchorRefusal =
            serde_json::from_str(document).expect("every anchor vector is a legal failure");
        assert_eq!(
            serde_json::to_string(&refusal).expect("a failure serializes"),
            document,
            "{}: rewritten differently",
            text(row, "note")
        );
        assert_eq!(row["enumerated"], Value::Bool(false), "{}", text(row, "note"));
        let members: Vec<String> = serde_json::from_str::<Value>(document)
            .expect("one object")
            .as_object()
            .expect("an object")
            .keys()
            .cloned()
            .collect();
        assert_eq!(members, vec!["failure".to_owned(), "root_path".to_owned()]);
    }
}

#[test]
fn a_result_from_another_request_is_rejected() {
    let asked: FindPagesContainingPhraseCommand =
        serde_json::from_str(r#"{"phrase":"annual report","root_path":"/content/example"}"#)
            .expect("a legal command");
    let own = FindPagesContainingPhraseResult::new(
        vec![PageMatch {
            repository_path: RepositoryPath::parse("/content/example/en").expect("a legal path"),
            title: None,
        }],
        None,
    )
    .expect("an ordered page");
    assert_eq!(own.require_answers(&asked), Ok(()));

    let elsewhere = FindPagesContainingPhraseResult::new(
        vec![PageMatch {
            repository_path: RepositoryPath::parse("/content/examples").expect("a legal path"),
            title: None,
        }],
        None,
    )
    .expect("an ordered page");
    assert_eq!(
        elsewhere.require_answers(&asked),
        Err(DiscoveryResultFailure::NotThisRequest),
        "containment is by segment, so a longer sibling name is not inside the anchor"
    );
}

#[test]
fn a_budget_that_runs_out_leaves_no_page_behind() {
    let vectors: Vec<Value> =
        rows(SCENARIOS).into_iter().filter(|row| text(row, "kind") == "budget").collect();
    assert_eq!(vectors.len(), 5, "all five common discriminators");
    for row in &vectors {
        assert_eq!(row["publishes_matches"], Value::Bool(false), "{}", text(row, "note"));
        assert_eq!(row["publishes_token"], Value::Bool(false), "{}", text(row, "note"));
    }
}

#[test]
fn every_named_bound_comes_from_the_manifest_rather_than_from_here() {
    let contract = slingshot_domain::command::command_identity::CommandContract::embedded();
    assert_eq!(maximum_search_phrase_bytes(), contract.limit("maximum_search_phrase_bytes"));
    assert_eq!(maximum_page_title_bytes(), contract.limit("maximum_page_title_bytes"));
}
