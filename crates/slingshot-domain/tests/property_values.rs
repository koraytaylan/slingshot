//! The repository's value model, proved lossless.
//!
//! Two properties are worth separating in what follows. A value *round trips*
//! when the bytes that came in are the bytes that go back out, which is what
//! keeps a Decimal's scale and a Date's spelling from being quietly rewritten.
//! A value *compares* by what it means, which is why `1.50` and `1.5` are equal
//! numbers while remaining different Decimals. A model that had only one of
//! those two would either lose information or answer arithmetic wrongly.

use serde_json::Value;
use slingshot_domain::command::property_value::{
    DateTimeString, DecimalString, PropertyScalarValue, PropertyValue, PropertyValueFailure,
    maximum_date_time_fraction_digits, maximum_decimal_bytes, maximum_decimal_fraction_digits,
    maximum_decimal_integer_digits, maximum_property_string_bytes, maximum_property_value_items,
};

/// Vectors this test reads.
const FIXTURE: &str = include_str!("fixtures/commands/property-values.jsonl");

/// Every refusal the fixture can name, beside the variant that produces it.
const DECLARED_REFUSALS: &[(&str, PropertyValueFailure)] = &[
    ("UnknownType", PropertyValueFailure::UnknownType),
    ("UnknownCardinality", PropertyValueFailure::UnknownCardinality),
    ("TypeMismatch", PropertyValueFailure::TypeMismatch),
    ("StringTooLong", PropertyValueFailure::StringTooLong),
    ("IntegerNotMinimal", PropertyValueFailure::IntegerNotMinimal),
    ("IntegerOutOfRange", PropertyValueFailure::IntegerOutOfRange),
    ("DecimalNotCanonical", PropertyValueFailure::DecimalNotCanonical),
    ("DecimalTooLong", PropertyValueFailure::DecimalTooLong),
    ("DateTimeNotCanonical", PropertyValueFailure::DateTimeNotCanonical),
    ("DateTimeNotACalendarDay", PropertyValueFailure::DateTimeNotACalendarDay),
    ("ListEmpty", PropertyValueFailure::ListEmpty),
    ("ListNotHomogeneous", PropertyValueFailure::ListNotHomogeneous),
    ("ListTooLong", PropertyValueFailure::ListTooLong),
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

/// Returns every sentence a value refusal can render as.
fn every_refusal_rendering() -> Vec<String> {
    DECLARED_REFUSALS.iter().map(|(_, failure)| failure.to_string()).collect()
}

/// Checks one vector against the type it names.
fn check<Parsed: serde::de::DeserializeOwned + std::fmt::Debug>(row: &Value) {
    let document = text(row, "document");
    let note = text(row, "note");
    let outcome = serde_json::from_str::<Parsed>(document);
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
        (Some(false), Ok(value)) => panic!("{note}: accepted as {value:?}"),
        (None, _) => panic!("{note}: the fixture states whether it is accepted"),
    }
}

#[test]
fn every_scalar_vector_lands_where_the_fixture_says_it_does() {
    let vectors = rows("scalar");
    assert!(vectors.len() >= 60, "every type is proved at both edges of every bound");
    for row in &vectors {
        check::<PropertyScalarValue>(row);
    }
}

#[test]
fn every_property_vector_lands_where_the_fixture_says_it_does() {
    let vectors = rows("property");
    assert!(vectors.len() >= 18, "both cardinalities are proved, and what neither accepts");
    for row in &vectors {
        check::<PropertyValue>(row);
    }
}

#[test]
fn every_accepted_value_writes_itself_back_byte_for_byte() {
    for kind in ["scalar", "property"] {
        for row in rows(kind).iter().filter(|row| row["accepted"] == Value::Bool(true)) {
            let document = text(row, "document");
            let note = text(row, "note");
            let written = match kind {
                "scalar" => rewrite::<PropertyScalarValue>(document),
                _ => rewrite::<PropertyValue>(document),
            };
            assert_eq!(written, document, "{note}: rewritten differently");
        }
    }
}

/// Parses one document and writes it back.
fn rewrite<Parsed: serde::de::DeserializeOwned + serde::Serialize>(document: &str) -> String {
    let parsed: Parsed = serde_json::from_str(document).expect("the fixture says this is accepted");
    serde_json::to_string(&parsed).expect("a valid value serializes")
}

#[test]
fn a_decimal_keeps_its_scale_while_comparing_as_a_number() {
    let written = DecimalString::new("1.50").expect("a legal decimal");
    let shorter = DecimalString::new("1.5").expect("a legal decimal");
    assert_eq!(written.as_text(), "1.50", "the scale is part of the value");
    assert_eq!(shorter.as_text(), "1.5");
    assert_ne!(written, shorter, "they are different Decimals");
    assert!(written.compare(&shorter).is_eq(), "and the same number");
}

#[test]
fn every_comparison_vector_agrees_with_the_model() {
    let vectors = rows("comparison");
    assert!(vectors.len() >= 18, "every ordered type and every unordered one is covered");
    for row in &vectors {
        let left = scalar_of(&row["left"]);
        let right = scalar_of(&row["right"]);
        let note = text(row, "note");
        assert_eq!(
            left.equals(&right),
            row["equal"].as_bool().expect("every vector states equality"),
            "{note}"
        );
        let expected = text(row, "order");
        let ordering = left.compare(&right);
        match (expected, ordering) {
            ("less", Some(std::cmp::Ordering::Less)) => (),
            ("greater", Some(std::cmp::Ordering::Greater)) => (),
            ("equal", Some(std::cmp::Ordering::Equal)) => (),
            ("incomparable" | "unordered", None) => (),
            (_, actual) => panic!("{note}: expected {expected}, got {actual:?}"),
        }
        if expected != "incomparable" && expected != "unordered" {
            assert_eq!(
                right.compare(&left).map(std::cmp::Ordering::reverse),
                ordering,
                "{note}: the comparison disagrees with itself reversed"
            );
        }
    }
}

/// Returns the scalar one fixture member spells.
fn scalar_of(value: &Value) -> PropertyScalarValue {
    serde_json::from_value(value.clone()).expect("every comparison operand is a legal scalar")
}

#[test]
fn an_ordered_comparison_refuses_a_path_and_a_list() {
    let path = PropertyScalarValue::Path(
        slingshot_domain::command::repository_path::RepositoryPropertyPath::parse("/content/a")
            .expect("a legal path"),
    );
    assert!(!path.is_ordered(), "a path is an address rather than a quantity");
    assert_eq!(path.compare(&path), None, "so it has no order, even against itself");
    assert!(path.equals(&path), "though it does equal itself");

    let list = PropertyValue::multiple(vec![
        PropertyScalarValue::integer("1").expect("a legal integer"),
        PropertyScalarValue::integer("2").expect("a legal integer"),
    ])
    .expect("a homogeneous list");
    let reversed = PropertyValue::multiple(vec![
        PropertyScalarValue::integer("2").expect("a legal integer"),
        PropertyScalarValue::integer("1").expect("a legal integer"),
    ])
    .expect("a homogeneous list");
    assert!(!list.equals(&reversed), "a JCR multi-value is ordered, so order is part of equality");
    assert!(list.equals(&list));
    assert!(
        !list.equals(&PropertyValue::Single(
            PropertyScalarValue::integer("1").expect("a legal integer")
        )),
        "a one-element list is not the scalar it contains"
    );
}

#[test]
fn every_named_bound_comes_from_the_manifest_rather_than_from_here() {
    let contract = slingshot_domain::command::command_identity::CommandContract::embedded();
    assert_eq!(maximum_property_string_bytes(), contract.limit("maximum_property_string_bytes"));
    assert_eq!(maximum_decimal_bytes(), contract.limit("maximum_decimal_bytes"));
    assert_eq!(maximum_decimal_integer_digits(), contract.limit("maximum_decimal_integer_digits"));
    assert_eq!(
        maximum_decimal_fraction_digits(),
        contract.limit("maximum_decimal_fraction_digits")
    );
    assert_eq!(
        maximum_date_time_fraction_digits(),
        contract.limit("maximum_date_time_fraction_digits")
    );
    assert_eq!(maximum_property_value_items(), contract.limit("maximum_property_value_items"));
}

#[test]
fn an_instant_has_exactly_one_spelling() {
    let whole = DateTimeString::new("2026-08-30T12:00:00Z").expect("a legal instant");
    assert_eq!(
        DateTimeString::new("2026-08-30T12:00:00.000Z"),
        Err(PropertyValueFailure::DateTimeNotCanonical),
        "the same instant with zero milliseconds written out is a second spelling"
    );
    let fractional = DateTimeString::new("2026-08-30T12:00:00.001Z").expect("a legal instant");
    assert!(whole.compare(&fractional).is_lt(), "one millisecond apart, in the right order");
    assert_eq!(whole.as_text(), "2026-08-30T12:00:00Z", "and neither was rewritten");
    assert_eq!(fractional.as_text(), "2026-08-30T12:00:00.001Z");
}

#[test]
fn this_model_can_represent_no_absence_and_no_deletion() {
    let single = serde_json::to_string(&PropertyValue::Single(
        PropertyScalarValue::text("widget").expect("a legal string"),
    ))
    .expect("a property serializes");
    assert!(!single.contains("null"), "there is no null to write");
    for absent in [
        r#"{"cardinality":"single","value":null}"#,
        r#"{"cardinality":"single","value":{"type":"null"}}"#,
        r#"{"cardinality":"single","value":{"type":"string","value":null}}"#,
        r#"{"cardinality":"delete","value":{"type":"string","value":"widget"}}"#,
    ] {
        assert!(
            serde_json::from_str::<PropertyValue>(absent).is_err(),
            "{absent} names an absence this model does not have"
        );
    }
}
