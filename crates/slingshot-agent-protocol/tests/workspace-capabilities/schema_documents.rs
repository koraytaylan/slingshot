//! Probe for the schema-documents capability.
//!
//! Requires a derived schema that declares the 2020-12 dialect, keeps renamed
//! property spellings, and marks an optional property as not required.

use schemars::{JsonSchema, schema_for};
use serde::Serialize;

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SubmitCommand {
    command_name: String,
    result_window: Option<u32>,
}

#[test]
fn a_derived_schema_declares_the_current_dialect_and_its_properties() {
    let schema = schema_for!(SubmitCommand);
    let document = serde_json::to_value(&schema).expect("the schema renders");
    let dialect = document["$schema"].as_str().expect("the schema names its dialect");
    assert!(dialect.contains("2020-12"), "{dialect}");
    let properties = document["properties"].as_object().expect("the schema lists properties");
    assert!(properties.contains_key("commandName"), "{properties:?}");
    assert!(properties.contains_key("resultWindow"), "{properties:?}");
    let required: Vec<&str> = document["required"]
        .as_array()
        .expect("required")
        .iter()
        .filter_map(|value| value.as_str())
        .collect();
    assert_eq!(required, vec!["commandName"]);
    assert_eq!(document["additionalProperties"], serde_json::Value::Bool(false));
}
