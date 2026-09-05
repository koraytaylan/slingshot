//! The complete event envelope has one closed schema, not just an identity projection.
use slingshot_agent_protocol::job_event_document::{JobEventDocument, SCHEMA};

#[test]
fn required_envelope_and_optional_counters_match_serialization() {
    let schema: serde_json::Value = serde_json::from_str(SCHEMA).unwrap();
    let wire = serde_json::json!({"agent_event_store_generation":7,
        "agent_operation_identifier":"a".repeat(64), "daemon_subscription_identifier":"subscription",
        "sling_job_identifier":"physical", "kind":"progress", "state":"running", "sequence":2});
    let document: JobEventDocument = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(serde_json::to_value(&document).unwrap(), wire);
    let required: std::collections::BTreeSet<_> = schema["required"]
        .as_array()
        .unwrap()
        .iter()
        .map(|field| field.as_str().unwrap())
        .collect();
    let keys: std::collections::BTreeSet<_> =
        wire.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(required, keys);
    assert_eq!(schema["additionalProperties"], false);
    assert_eq!(schema["properties"].as_object().unwrap().len(), 10);
    for field in ["attempt", "progress", "terminal"] {
        let mut null = wire.clone();
        null[field] = serde_json::Value::Null;
        assert!(serde_json::from_value::<JobEventDocument>(null).is_err());
    }
    assert_eq!(format!("{document:?}"), "JobEventDocument([redacted])");
}
