//! The general discovery command, proved ordered and proved literal.
//!
//! Three things carry the weight. Results are strictly ascending by repository
//! path bytes with no path twice, because that ordering is what makes a
//! continuation token mean anything - a page resumes after its last path, and
//! an unordered page has no "after". An absent property answers no to every
//! operator, `NotEquals` included, because a node with no such property has not
//! been shown to differ from the value. And a string that spells a query
//! language stays a value.

use serde_json::Value;
use slingshot_domain::command::property_value::PropertyValue;
use slingshot_domain::command::query_paths::{
    AnchorRefusal, DiscoveryResultFailure, PathMatch, QueryPathsCommand, QueryPathsResult,
    require_strictly_ascending,
};
use slingshot_domain::command::repository_path::RepositoryPath;
use slingshot_domain::command::result_window::ContinuationToken;
use slingshot_domain::command::search_predicate::{ObservedProperty, PropertyPredicate};

/// Commands this test reads.
const COMMANDS: &str = include_str!("fixtures/commands/query_paths/commands.jsonl");

/// Results this test reads.
const RESULTS: &str = include_str!("fixtures/commands/query_paths/results.jsonl");

/// Anchor failures this test reads.
const ANCHORS: &str = include_str!("fixtures/commands/query_paths/anchors.jsonl");

/// Scenarios this test reads.
const SCENARIOS: &str = include_str!("fixtures/commands/query_paths/scenarios.jsonl");

/// Every refusal the fixtures can name, beside the variant that produces it.
const DECLARED_REFUSALS: &[(&str, DiscoveryResultFailure)] = &[
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
        .or_else(|| panic!("the fixture names a refusal this test does not know: {reason}"))
}

/// Returns every sentence a discovery refusal can render as.
fn every_refusal_rendering() -> Vec<String> {
    DECLARED_REFUSALS.iter().map(|(_, failure)| failure.to_string()).collect()
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
    assert!(vectors.len() >= 18, "every combination and every refusal is covered");
    for row in &vectors {
        check::<QueryPathsCommand>(row);
    }
}

#[test]
fn every_accepted_command_writes_itself_back_byte_for_byte() {
    for row in rows(COMMANDS).iter().filter(|row| row["accepted"] == Value::Bool(true)) {
        let document = text(row, "document");
        let command: QueryPathsCommand =
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
fn an_omitted_window_and_omitted_predicates_resolve_rather_than_failing() {
    let command: QueryPathsCommand = serde_json::from_str(r#"{"root_path":"/content"}"#)
        .expect("an anchor alone is a legal question");
    assert_eq!(command.primary_node_type, None);
    assert!(command.predicates().is_empty(), "no predicate is a legal question");
    assert_eq!(
        command.resolved_window(),
        slingshot_domain::command::result_window::ResultWindow::omitted()
    );
    assert_eq!(
        serde_json::to_string(&command).expect("a command serializes"),
        r#"{"root_path":"/content"}"#,
        "and the request echoes what it was asked, not what it resolved to"
    );
}

#[test]
fn every_result_vector_lands_where_the_fixture_says_it_does() {
    for row in &rows(RESULTS) {
        check::<QueryPathsResult>(row);
    }
}

#[test]
fn a_page_is_strictly_ascending_by_bytes_and_never_repeats_a_path() {
    let path = |spelling: &str| RepositoryPath::parse(spelling).expect("a legal path");
    let ascending = vec![path("/content/A"), path("/content/a"), path("/content/b")];
    assert_eq!(require_strictly_ascending(&ascending), Ok(()));

    for wrong in [
        vec![path("/content/a"), path("/content/a")],
        vec![path("/content/b"), path("/content/a")],
        vec![path("/content/a"), path("/content/A")],
    ] {
        assert_eq!(
            require_strictly_ascending(&wrong),
            Err(DiscoveryResultFailure::NotStrictlyAscending),
            "{wrong:?} is not strictly ascending by bytes"
        );
    }
    assert_eq!(
        require_strictly_ascending(std::iter::empty()),
        Ok(()),
        "an empty page is trivially ordered"
    );
}

#[test]
fn every_anchor_failure_is_the_closed_shape_the_contract_declares() {
    for row in rows(ANCHORS).iter().filter(|row| text(row, "kind") == "anchor") {
        let document = text(row, "document");
        let note = text(row, "note");
        let refusal: AnchorRefusal =
            serde_json::from_str(document).expect("every anchor vector is a legal failure");
        assert_eq!(
            serde_json::to_string(&refusal).expect("a failure serializes"),
            document,
            "{note}: rewritten differently"
        );
        let members: Vec<String> = serde_json::from_str::<Value>(document)
            .expect("one object")
            .as_object()
            .expect("an object")
            .keys()
            .cloned()
            .collect();
        assert_eq!(
            members,
            vec!["failure".to_owned(), "root_path".to_owned()],
            "{note}: an anchor failure carries no matches and no token"
        );
    }
    for row in rows(ANCHORS).iter().filter(|row| text(row, "kind") == "anchor_refused") {
        assert!(
            serde_json::from_str::<AnchorRefusal>(text(row, "document")).is_err(),
            "{}: accepted",
            text(row, "note")
        );
    }
}

#[test]
fn a_result_or_a_refusal_from_another_request_is_rejected() {
    let asked: QueryPathsCommand =
        serde_json::from_str(r#"{"root_path":"/content/example"}"#).expect("a legal command");
    let own = QueryPathsResult::new(
        vec![PathMatch {
            repository_path: RepositoryPath::parse("/content/example/en").expect("a legal path"),
        }],
        None,
    )
    .expect("an ordered page");
    assert_eq!(own.require_answers(&asked), Ok(()));

    let outside = QueryPathsResult::new(
        vec![PathMatch {
            repository_path: RepositoryPath::parse("/content/other").expect("a legal path"),
        }],
        None,
    )
    .expect("an ordered page");
    assert_eq!(
        outside.require_answers(&asked),
        Err(DiscoveryResultFailure::NotThisRequest),
        "a match outside the anchor is not this request's"
    );

    let adjacent = QueryPathsResult::new(
        vec![PathMatch {
            repository_path: RepositoryPath::parse("/content/examples").expect("a legal path"),
        }],
        None,
    )
    .expect("an ordered page");
    assert_eq!(
        adjacent.require_answers(&asked),
        Err(DiscoveryResultFailure::NotThisRequest),
        "containment is by segment, so a longer sibling name is not inside the anchor"
    );

    let elsewhere: AnchorRefusal =
        serde_json::from_str(r#"{"failure":"root_not_found","root_path":"/content/other"}"#)
            .expect("a legal refusal");
    assert_eq!(
        elsewhere.require_answers(&asked),
        Err(DiscoveryResultFailure::NotThisRequest),
        "and the echoed anchor is the only thing telling two refusals apart"
    );
}

#[test]
fn the_repository_root_contains_everything() {
    let asked: QueryPathsCommand =
        serde_json::from_str(r#"{"root_path":"/"}"#).expect("a legal command");
    let page = QueryPathsResult::new(
        vec![PathMatch {
            repository_path: RepositoryPath::parse("/content/example").expect("a legal path"),
        }],
        None,
    )
    .expect("an ordered page");
    assert_eq!(page.require_answers(&asked), Ok(()));
}

#[test]
fn an_absent_property_answers_no_to_every_operator() {
    for row in rows(SCENARIOS).iter().filter(|row| text(row, "kind") == "matching") {
        let note = text(row, "note");
        let mut document = serde_json::Map::new();
        document.insert("operator".to_owned(), Value::from(text(row, "operator")));
        document.insert("property_path".to_owned(), Value::from("jcr:content/jcr:title"));
        for (name, value) in row["fields"].as_object().expect("a field map") {
            document.insert(name.clone(), value.clone());
        }
        let predicate: PropertyPredicate = serde_json::from_value(Value::Object(document))
            .unwrap_or_else(|failure| panic!("{note}: {failure}"));
        let observed = observed_of(&row["resolved"]);
        assert_eq!(
            predicate.matches(observed.as_ref()),
            row["matches"].as_bool().expect("every vector states its answer"),
            "{note}"
        );
    }
}

/// Returns the observation one fixture member describes.
fn observed_of(row: &Value) -> Option<ObservedProperty> {
    match text(row, "state") {
        "absent" => None,
        _ => Some(ObservedProperty::Held(
            serde_json::from_value::<PropertyValue>(row["value"].clone())
                .expect("every held observation is a legal property value"),
        )),
    }
}

#[test]
fn predicates_combine_with_logical_and() {
    for row in rows(SCENARIOS).iter().filter(|row| text(row, "kind") == "combination") {
        let answers: Vec<bool> = row["answers"]
            .as_array()
            .expect("every combination lists its answers")
            .iter()
            .map(|answer| answer.as_bool().expect("a Boolean"))
            .collect();
        assert_eq!(
            answers.iter().all(|answer| *answer),
            row["matches"].as_bool().expect("every vector states its verdict"),
            "{}",
            text(row, "note")
        );
    }
}

#[test]
fn a_budget_that_runs_out_leaves_no_page_behind() {
    let vectors: Vec<Value> =
        rows(SCENARIOS).into_iter().filter(|row| text(row, "kind") == "budget").collect();
    assert_eq!(vectors.len(), 5, "all five common discriminators");
    for row in &vectors {
        let note = text(row, "note");
        assert_eq!(row["publishes_matches"], Value::Bool(false), "{note}");
        assert_eq!(row["publishes_token"], Value::Bool(false), "{note}");
        let quoted = format!("\"{}\"", text(row, "budget"));
        assert!(
            declared_budgets().contains(&quoted),
            "{note}: not one of the five the shared budget declares"
        );
    }
}

/// Returns every budget literal the shared discovery budget declares.
///
/// Taken from that type rather than written down again, so a sixth budget or a
/// renamed one fails here instead of drifting quietly.
fn declared_budgets() -> Vec<String> {
    use slingshot_domain::command::discovery_budget::DiscoveryBudget;

    [
        DiscoveryBudget::CandidateNodes,
        DiscoveryBudget::PropertyValues,
        DiscoveryBudget::PropertyBytes,
        DiscoveryBudget::CriterionEvaluations,
        DiscoveryBudget::ExecutionDuration,
    ]
    .iter()
    .map(|budget| serde_json::to_string(budget).expect("a budget serializes"))
    .collect()
}

#[test]
fn a_string_that_spells_a_query_stays_a_value() {
    let statement = "SELECT * FROM [cq:Page] WHERE ISDESCENDANTNODE('/')";
    let document = format!(
        "{{\"property_predicates\":[{{\"operator\":\"equals\",\"property_path\":\"note\",\
         \"value\":{{\"cardinality\":\"single\",\"value\":{{\"type\":\"string\",\
         \"value\":{}}}}}}}],\"root_path\":\"/content\"}}",
        serde_json::to_string(statement).expect("a string serializes")
    );
    let command: QueryPathsCommand =
        serde_json::from_str(&document).expect("a query language is an ordinary string");
    let predicate = &command.predicates()[0];
    let holding = |value: &str| {
        Some(ObservedProperty::Held(PropertyValue::Single(
            slingshot_domain::command::property_value::PropertyScalarValue::text(value)
                .expect("a legal string"),
        )))
    };
    assert!(predicate.matches(holding(statement).as_ref()), "compared as the string it is");
    assert!(
        !predicate.matches(holding("SELECT * FROM [cq:Page]").as_ref()),
        "and not as a query that would match more"
    );
}

#[test]
fn a_token_survives_a_page_unchanged() {
    let spelling = "aGVhZGVy.cGF5bG9hZA.dGFn";
    let page = QueryPathsResult::new(
        Vec::new(),
        Some(ContinuationToken::new(spelling).expect("a shaped token")),
    )
    .expect("an empty page may still carry a token");
    let written = serde_json::to_string(&page).expect("a page serializes");
    assert_eq!(written, format!(r#"{{"matches":[],"next_continuation_token":"{spelling}"}}"#));
    let read: QueryPathsResult = serde_json::from_str(&written).expect("its own bytes parse");
    assert_eq!(read, page);
}
