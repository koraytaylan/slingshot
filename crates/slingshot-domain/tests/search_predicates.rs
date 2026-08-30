//! The structured predicate language, proved closed and proved typed.
//!
//! Two claims matter more than the rest. The first is that no string a caller
//! sends ever becomes syntax: a value that spells a JCR-SQL2 statement is an
//! ordinary string and is compared as one. The second is that a type is never
//! guessed from the shape of a JSON token, so `"1"` and `1` are different
//! questions and a mistyped predicate is refused rather than silently matching
//! nothing.

use serde_json::Value;
use slingshot_domain::command::property_value::{PropertyScalarValue, PropertyValue};
use slingshot_domain::command::repository_path::RelativePropertyPath;
use slingshot_domain::command::search_predicate::{
    DECLARED_OPERATORS, MembershipValues, ObservedProperty, OrderedScalarPropertyValue,
    PredicateFailure, PropertyPredicate, PropertyPredicates, maximum_property_predicate_values,
    maximum_property_predicates,
};

/// Vectors this test reads.
const FIXTURE: &str = include_str!("fixtures/commands/search-predicates.jsonl");

/// Every refusal the fixture can name, beside the variant that produces it.
const DECLARED_REFUSALS: &[(&str, PredicateFailure)] = &[
    ("UnknownOperator", PredicateFailure::UnknownOperator),
    ("FieldsDoNotMatchOperator", PredicateFailure::FieldsDoNotMatchOperator),
    ("ValuesEmpty", PredicateFailure::ValuesEmpty),
    ("ValuesNotUnique", PredicateFailure::ValuesNotUnique),
    ("ValuesNotHomogeneous", PredicateFailure::ValuesNotHomogeneous),
    ("ValuesTooMany", PredicateFailure::ValuesTooMany),
    ("ValueNotOrdered", PredicateFailure::ValueNotOrdered),
    ("TooManyPredicates", PredicateFailure::TooManyPredicates),
];

/// Name the fixture gives to the refusals the closed object makes on its own.
const CLOSED_OBJECT: &str = "ClosedObject";

/// Reads one row's string member.
fn text<'row>(row: &'row Value, member: &str) -> &'row str {
    row[member].as_str().unwrap_or_else(|| panic!("{member} is a string in {row}"))
}

/// Returns every fixture row of one kind.
fn rows(kind: &str) -> Vec<Value> {
    FIXTURE
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("every fixture line is one object"))
        .filter(|row| text(row, "kind") == kind)
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

/// Returns every sentence a predicate refusal can render as.
fn every_refusal_rendering() -> Vec<String> {
    DECLARED_REFUSALS.iter().map(|(_, failure)| failure.to_string()).collect()
}

#[test]
fn every_predicate_vector_lands_where_the_fixture_says_it_does() {
    let vectors = rows("predicate");
    assert!(vectors.len() >= 35, "every operator and every refusal is covered");
    for row in &vectors {
        let document = text(row, "document");
        let note = text(row, "note");
        let outcome = serde_json::from_str::<PropertyPredicate>(document);
        match (row["accepted"].as_bool(), outcome) {
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
            (Some(false), Ok(predicate)) => panic!("{note}: accepted as {predicate:?}"),
            (None, _) => panic!("{note}: the fixture states whether it is accepted"),
        }
    }
}

#[test]
fn every_accepted_predicate_writes_itself_back_byte_for_byte() {
    for row in rows("predicate").iter().filter(|row| row["accepted"] == Value::Bool(true)) {
        let document = text(row, "document");
        let predicate: PropertyPredicate =
            serde_json::from_str(document).expect("the fixture says this is accepted");
        assert_eq!(
            serde_json::to_string(&predicate).expect("a valid predicate serializes"),
            document,
            "{}: rewritten differently",
            text(row, "note")
        );
    }
}

#[test]
fn every_operator_this_language_declares_has_an_accepted_vector() {
    let accepted: Vec<PropertyPredicate> = rows("predicate")
        .iter()
        .filter(|row| row["accepted"] == Value::Bool(true))
        .map(|row| {
            serde_json::from_str(text(row, "document")).expect("the fixture says this is accepted")
        })
        .collect();
    for operator in DECLARED_OPERATORS {
        assert!(
            accepted.iter().any(|predicate| predicate.operator() == *operator),
            "{operator} has no accepted vector"
        );
    }
    assert_eq!(DECLARED_OPERATORS.len(), 10, "ten operators, and no eleventh");
}

#[test]
fn every_match_vector_answers_the_way_the_fixture_says() {
    let vectors = rows("match");
    assert!(vectors.len() >= 25, "each operator is answered both ways");
    for row in &vectors {
        let predicate: PropertyPredicate = serde_json::from_str(text(row, "document"))
            .expect("every match vector names a legal predicate");
        let observed = observed_of(&row["observed"]);
        assert_eq!(
            predicate.matches(observed.as_ref()),
            row["matches"].as_bool().expect("every vector states its answer"),
            "{}",
            text(row, "note")
        );
    }
}

/// Returns the observation one fixture member describes.
fn observed_of(row: &Value) -> Option<ObservedProperty> {
    match text(row, "state") {
        "absent" => None,
        "empty_multiple" => Some(ObservedProperty::EmptyMultiple),
        "held" => Some(ObservedProperty::Held(
            serde_json::from_value::<PropertyValue>(row["value"].clone())
                .expect("every held observation is a legal property value"),
        )),
        other => panic!("the fixture describes an observation this test does not know: {other}"),
    }
}

#[test]
fn a_search_composes_a_bounded_number_of_predicates() {
    let one = || PropertyPredicate::Exists {
        property_path: RelativePropertyPath::parse("title").expect("a legal path"),
    };
    for row in &rows("collection") {
        let count = usize::try_from(row["count"].as_u64().expect("a count")).expect("addressable");
        let outcome = PropertyPredicates::new(vec![one(); count]);
        assert_eq!(
            outcome.is_ok(),
            row["accepted"].as_bool().expect("the fixture states whether it is accepted"),
            "{}",
            text(row, "note")
        );
        if let Err(failure) = outcome {
            assert_eq!(failure, PredicateFailure::TooManyPredicates);
        }
    }
}

#[test]
fn a_predicate_resolves_exactly_the_property_it_names() {
    let nested =
        RelativePropertyPath::parse("jcr:content/metadata/dc:title").expect("a legal nested path");
    let predicate = PropertyPredicate::Exists { property_path: nested.clone() };
    let asked = std::cell::RefCell::new(Vec::new());
    let composed = PropertyPredicates::new(vec![predicate]).expect("one predicate");
    let held = composed.all_match(|path| {
        asked.borrow_mut().push(path.as_text().to_owned());
        None
    });
    assert!(!held, "an absent property answers no");
    assert_eq!(
        asked.into_inner(),
        vec![nested.as_text().to_owned()],
        "exactly the named path was resolved, with no descendant search and no fallback"
    );
}

#[test]
fn a_string_that_spells_a_query_stays_a_string() {
    let statement = "SELECT * FROM [cq:Page] WHERE ISDESCENDANTNODE('/')";
    let predicate = PropertyPredicate::Equals {
        property_path: RelativePropertyPath::parse("query").expect("a legal path"),
        value: PropertyValue::Single(PropertyScalarValue::text(statement).expect("a legal string")),
    };
    let holding = |value: &str| {
        Some(ObservedProperty::Held(PropertyValue::Single(
            PropertyScalarValue::text(value).expect("a legal string"),
        )))
    };
    assert!(predicate.matches(holding(statement).as_ref()), "it is compared as the string it is");
    assert!(
        !predicate.matches(holding("SELECT * FROM [cq:Page]").as_ref()),
        "and not as a query that would match more"
    );
}

#[test]
fn membership_uniqueness_is_by_value_rather_than_by_spelling() {
    let same_number = vec![
        scalar_of(r#"{"type":"decimal","value":"1.5"}"#),
        scalar_of(r#"{"type":"decimal","value":"1.50"}"#),
    ];
    assert_eq!(MembershipValues::new(same_number), Err(PredicateFailure::ValuesNotUnique));

    let different = vec![
        scalar_of(r#"{"type":"decimal","value":"1.5"}"#),
        scalar_of(r#"{"type":"decimal","value":"1.51"}"#),
    ];
    let values = MembershipValues::new(different).expect("two different numbers");
    assert!(values.contains(&scalar_of(r#"{"type":"decimal","value":"1.500"}"#)));
    assert!(!values.contains(&scalar_of(r#"{"type":"integer","value":"1"}"#)));
}

/// Returns the scalar one document spells.
fn scalar_of(document: &str) -> PropertyScalarValue {
    serde_json::from_str(document).expect("a legal scalar")
}

#[test]
fn an_ordered_comparison_cannot_be_built_over_an_address() {
    let path = PropertyScalarValue::Path(
        slingshot_domain::command::repository_path::RepositoryPropertyPath::parse("/content/a")
            .expect("a legal path"),
    );
    assert_eq!(
        OrderedScalarPropertyValue::new(path),
        Err(PredicateFailure::ValueNotOrdered),
        "the refusal happens where the predicate is built, not where it is evaluated"
    );
}

#[test]
fn every_named_bound_comes_from_the_manifest_rather_than_from_here() {
    let contract = slingshot_domain::command::command_identity::CommandContract::embedded();
    assert_eq!(
        maximum_property_predicate_values(),
        contract.limit("maximum_property_predicate_values")
    );
    assert_eq!(maximum_property_predicates(), contract.limit("maximum_property_predicates"));
}
