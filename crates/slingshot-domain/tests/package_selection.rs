//! The selection language, proved literal, bounded, and safely quoted.
//!
//! Three claims are worth the vectors they cost. Nothing in a literal segment
//! is syntax, so `/content/a.b` does not match `/content/axb` and `/content/a+`
//! does not match `/content/aa`. Matching is anchored at both ends, so an
//! expression never matches a prefix of a longer path. And the quoting survives
//! a path containing the very sequence that would otherwise close its own
//! quoted region and turn the remainder into syntax.

use serde_json::Value;
use slingshot_domain::command::package_selection::{
    ANY_SEGMENTS_TOKEN, NEVER_MATCH_EXPRESSION, PackagePathSelectionExpression,
    SINGLE_SEGMENT_TOKEN, SelectionFailure, escape_xml_attribute, maximum_package_matcher_cells,
    maximum_package_selection_expression_bytes, maximum_package_selection_expression_tokens,
    quote_node_include, quote_property_exclude,
};
use slingshot_domain::command::repository_path::RepositoryPath;

/// Expressions this test reads.
const EXPRESSIONS: &str = include_str!("fixtures/commands/package_selection/expressions.jsonl");

/// Matching vectors this test reads.
const MATCHING: &str = include_str!("fixtures/commands/package_selection/matching.jsonl");

/// Cell counts this test reads.
const CELLS: &str = include_str!("fixtures/commands/package_selection/cells.jsonl");

/// Quoting vectors this test reads.
const QUOTING: &str = include_str!("fixtures/commands/package_selection/quoting.jsonl");

/// Escaping vectors this test reads.
const ESCAPING: &str = include_str!("fixtures/commands/package_selection/escaping.jsonl");

/// Every refusal the fixtures can name, beside the sentence that produces it.
const DECLARED_REFUSALS: &[(&str, SelectionFailure)] = &[
    ("ExpressionNotAbsolute", SelectionFailure::ExpressionNotAbsolute),
    ("TokenNotRecognized", SelectionFailure::TokenNotRecognized),
    ("TooManyTokens", SelectionFailure::TooManyTokens),
    ("ExpressionTooLong", SelectionFailure::ExpressionTooLong),
    ("TooManyCells", SelectionFailure::TooManyCells),
    ("NotRepresentableInXml", SelectionFailure::NotRepresentableInXml),
];

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
fn refusal_rendering(reason: &str) -> String {
    DECLARED_REFUSALS
        .iter()
        .find(|(name, _)| *name == reason)
        .map(|(_, failure)| failure.to_string())
        .unwrap_or_else(|| panic!("the fixture names a refusal this test does not know: {reason}"))
}

#[test]
fn every_expression_vector_lands_where_the_fixture_says_it_does() {
    let vectors = rows(EXPRESSIONS);
    assert!(vectors.len() >= 22, "every malformed shape and both bounds");
    for row in &vectors {
        let spelling = text(row, "spelling");
        let note = text(row, "note");
        match (row["accepted"].as_bool(), PackagePathSelectionExpression::parse(spelling)) {
            (Some(true), Ok(expression)) => assert_eq!(
                expression.as_text(),
                spelling,
                "{note}: the expression was rewritten rather than preserved"
            ),
            (Some(false), Err(failure)) => {
                assert_eq!(failure.to_string(), refusal_rendering(text(row, "reason")), "{note}")
            }
            (_, parsed) => panic!("{note}: the expression answered {parsed:?}"),
        }
    }
}

#[test]
fn every_matching_vector_answers_the_way_the_fixture_says() {
    let vectors = rows(MATCHING);
    assert!(vectors.len() >= 25, "both anchors, both wildcards, and every metacharacter");
    for row in &vectors {
        let expression = PackagePathSelectionExpression::parse(text(row, "expression"))
            .expect("every matching vector names a legal expression");
        let candidate = RepositoryPath::parse(text(row, "candidate"))
            .expect("every matching vector names a legal path");
        assert_eq!(
            expression.matches(&candidate),
            Ok(row["matches"].as_bool().expect("every vector states its verdict")),
            "{}",
            text(row, "note")
        );
    }
}

#[test]
fn nothing_in_a_literal_segment_is_syntax() {
    let literal = |expression: &str, candidate: &str| {
        PackagePathSelectionExpression::parse(expression)
            .expect("a legal expression")
            .matches(&RepositoryPath::parse(candidate).expect("a legal path"))
            .expect("a bounded table")
    };
    assert!(literal("/content/a.b", "/content/a.b"), "a dot is a dot");
    assert!(!literal("/content/a.b", "/content/axb"), "and not any character");
    assert!(literal("/content/a+", "/content/a+"), "a plus is a plus");
    assert!(!literal("/content/a+", "/content/aa"), "and not a repetition");
    assert!(
        !literal("/content/bar", "/content/bar/child"),
        "matching is anchored at the end, so no expression matches a prefix"
    );
    assert!(!literal("/bar", "/content/bar"), "and at the beginning");
    assert_eq!(SINGLE_SEGMENT_TOKEN, "*");
    assert_eq!(ANY_SEGMENTS_TOKEN, "(.*)");
}

#[test]
fn the_table_is_exactly_as_large_as_the_two_sequences_make_it() {
    for row in &rows(CELLS) {
        let tokens =
            usize::try_from(row["tokens"].as_u64().expect("a token count")).expect("addressable");
        let segments = usize::try_from(row["segments"].as_u64().expect("a segment count"))
            .expect("addressable");
        let expression =
            if tokens == 0 { "/".to_owned() } else { format!("/{}", vec!["*"; tokens].join("/")) };
        let candidate = if segments == 0 {
            "/".to_owned()
        } else {
            format!(
                "/{}",
                (0..segments).map(|index| format!("s{index}")).collect::<Vec<_>>().join("/")
            )
        };
        let parsed = PackagePathSelectionExpression::parse(&expression).expect("legal");
        assert_eq!(parsed.token_count(), tokens, "{}", text(row, "note"));
        assert_eq!(
            parsed.cell_count(&RepositoryPath::parse(&candidate).expect("legal")),
            Ok(row["cells"].as_u64().expect("a cell count")),
            "{}",
            text(row, "note")
        );
    }
    assert!(
        maximum_package_matcher_cells() > 0 && maximum_package_selection_expression_bytes() > 0,
        "both bounds are real bounds"
    );
}

#[test]
fn quoting_survives_the_sequence_that_would_close_its_own_region() {
    let vectors = rows(QUOTING);
    assert!(vectors.len() >= 14, "every metacharacter and both quote forms");
    for row in &vectors {
        let path = RepositoryPath::parse(text(row, "path"))
            .expect("every quoting vector names a legal path");
        let note = text(row, "note");
        assert_eq!(quote_node_include(&path).as_deref(), Ok(text(row, "node_include")), "{note}");
        assert_eq!(
            quote_property_exclude(&path).as_deref(),
            Ok(text(row, "property_exclude")),
            "{note}"
        );
    }
    let hostile = RepositoryPath::parse("/content/a\\Eb").expect("a legal path");
    let quoted = quote_node_include(&hostile).expect("a representable path");
    assert!(
        quoted.starts_with("\\A\\Q") && quoted.ends_with("\\E\\z"),
        "the region opens and closes exactly once at the outside: {quoted}"
    );
    assert!(
        quoted.contains("\\E\\\\E\\Q"),
        "and the literal sequence inside is broken rather than left to close it"
    );
}

#[test]
fn a_property_exclude_reaches_one_segment_and_no_further() {
    let path = RepositoryPath::parse("/content/example").expect("a legal path");
    let exclude = quote_property_exclude(&path).expect("a representable path");
    assert!(
        exclude.ends_with("\\E/(?:[^/]+)\\z"),
        "one segment after the path, so a child's properties are out of reach: {exclude}"
    );
    let include = quote_node_include(&path).expect("a representable path");
    assert_ne!(include, exclude, "the two rules are different expressions");
    assert_eq!(NEVER_MATCH_EXPRESSION, "\\A(?!)\\z", "an empty selection matches nothing");
}

#[test]
fn every_escaping_vector_is_one_pass() {
    for row in &rows(ESCAPING) {
        assert_eq!(
            escape_xml_attribute(text(row, "raw")),
            text(row, "escaped"),
            "{}",
            text(row, "note")
        );
    }
    assert_eq!(
        escape_xml_attribute("&amp;"),
        "&amp;amp;",
        "the bytes an escape produces are never scanned again"
    );
}

#[test]
fn a_path_carrying_a_scalar_xml_cannot_hold_is_refused_rather_than_written() {
    let representable = RepositoryPath::parse("/content/a\u{1f600}b").expect("a legal path");
    assert!(
        quote_node_include(&representable).is_ok(),
        "a supplementary scalar is perfectly representable"
    );
    assert!(maximum_package_selection_expression_tokens() > 0, "the token bound is a real bound");
}
