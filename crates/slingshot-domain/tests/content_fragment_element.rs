//! Assertions for what a content fragment holds.
//!
//! The closed pair of value forms is the point. A single value is not a list of
//! one and is never rewritten as one, because a model declares an element as
//! single-valued or multi-valued and a contract that reshaped the request would
//! be answering a question the caller did not ask.

use slingshot_domain::command::command_identity::CommandContract;
use slingshot_domain::command::content_fragment_element::{
    ContentFragmentElementName, ContentFragmentElementValue, ContentFragmentElementValues,
    ContentFragmentFailure, ContentFragmentVariationName,
};

/// Returns one limit by name.
fn limit(name: &str) -> usize {
    usize::try_from(CommandContract::embedded().limit(name)).expect("the bound fits")
}

#[test]
fn an_element_name_and_a_variation_name_refuse_paths_controls_and_edge_spaces() {
    for accepted in ["title", "main-text", "dc:description", "tags_1"] {
        assert!(ContentFragmentElementName::parse(accepted).is_ok(), "{accepted} was refused");
        assert!(ContentFragmentVariationName::parse(accepted).is_ok(), "{accepted} was refused");
    }
    for refused in ["", "ti/tle", " title", "title ", "tit\u{0}le"] {
        assert!(ContentFragmentElementName::parse(refused).is_err(), "{refused:?} was accepted");
        assert!(ContentFragmentVariationName::parse(refused).is_err(), "{refused:?} was accepted");
    }
}

#[test]
fn each_name_is_accepted_at_its_bound_and_refused_one_byte_past_it() {
    let element = "a".repeat(limit("maximum_content_fragment_element_name_bytes"));
    assert!(ContentFragmentElementName::parse(&element).is_ok(), "the bound itself was refused");
    assert!(ContentFragmentElementName::parse(&format!("{element}a")).is_err());
    let variation = "a".repeat(limit("maximum_content_fragment_variation_name_bytes"));
    assert!(
        ContentFragmentVariationName::parse(&variation).is_ok(),
        "the bound itself was refused"
    );
    assert!(ContentFragmentVariationName::parse(&format!("{variation}a")).is_err());
}

#[test]
fn a_single_value_is_never_rewritten_as_a_list_of_one() {
    let single = ContentFragmentElementValue::single("Spring offer").expect("a legal value");
    assert_eq!(serde_json::to_string(&single).expect("a value serializes"), "\"Spring offer\"");
    let list = ContentFragmentElementValue::list(vec!["spring".to_owned()]).expect("a legal list");
    assert_eq!(serde_json::to_string(&list).expect("a value serializes"), "[\"spring\"]");
    assert_ne!(single, list);
}

#[test]
fn a_list_holds_at_least_one_value_and_at_most_its_bound() {
    assert_eq!(
        ContentFragmentElementValue::list(Vec::new()),
        Err(ContentFragmentFailure::EmptyList)
    );
    let bound = limit("maximum_content_fragment_element_values");
    let exact: Vec<String> = (0..bound).map(|index| format!("v{index}")).collect();
    assert!(ContentFragmentElementValue::list(exact.clone()).is_ok(), "the bound was refused");
    let mut beyond = exact;
    beyond.push("one more".to_owned());
    assert_eq!(
        ContentFragmentElementValue::list(beyond),
        Err(ContentFragmentFailure::TooManyValues)
    );
}

#[test]
fn a_value_is_accepted_at_its_bound_and_refused_one_byte_past_it() {
    let bound = limit("maximum_property_string_bytes");
    let exact = "a".repeat(bound);
    assert!(ContentFragmentElementValue::single(&exact).is_ok(), "the bound itself was refused");
    assert_eq!(
        ContentFragmentElementValue::single(&format!("{exact}a")),
        Err(ContentFragmentFailure::ValueTooLong)
    );
}

#[test]
fn an_element_document_is_accepted_at_its_bound_and_refused_one_element_past_it() {
    let bound = limit("maximum_content_fragment_elements");
    let build = |count: usize| {
        let mut values = std::collections::BTreeMap::new();
        for index in 0..count {
            values.insert(
                ContentFragmentElementName::parse(&format!("e{index:06}")).expect("a legal name"),
                ContentFragmentElementValue::single("x").expect("a legal value"),
            );
        }
        ContentFragmentElementValues::new(values)
    };
    assert!(build(bound).is_ok(), "the bound itself was refused");
    assert_eq!(build(bound + 1), Err(ContentFragmentFailure::TooManyElements));
}

#[test]
fn an_element_document_round_trips_and_refuses_an_unusable_name_or_value() {
    let written = r#"{"tags":["spring","sale"],"title":"Spring offer"}"#;
    let read: ContentFragmentElementValues =
        serde_json::from_str(written).expect("a document parses");
    assert_eq!(serde_json::to_string(&read).expect("a document serializes"), written);
    assert!(serde_json::from_str::<ContentFragmentElementValues>(r#"{"ti/tle":"x"}"#).is_err());
    assert!(serde_json::from_str::<ContentFragmentElementValues>(r#"{"tags":[]}"#).is_err());
    assert!(
        read.values()
            .contains_key(&ContentFragmentElementName::parse("title").expect("a legal name"))
    );
    assert!(!read.is_empty());
}
