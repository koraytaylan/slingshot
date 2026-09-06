//! Probe for the JSON Schema validation capability.

#[test]
fn json_schema_documents_are_validated_by_the_pinned_engine() {
    let schema = serde_json::json!({"type": "object", "required": ["ok"]});
    let validator = jsonschema::draft202012::new(&schema).expect("the schema compiles");
    assert!(validator.is_valid(&serde_json::json!({"ok": true})));
    assert!(!validator.is_valid(&serde_json::json!({"missing": true})));
}
