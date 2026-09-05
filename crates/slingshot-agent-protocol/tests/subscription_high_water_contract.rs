//! Closed high-water schema inventory and serialization contract.
use slingshot_agent_protocol::subscription_high_water::{SCHEMA, SubscriptionHighWater};
use slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract;

#[test]
fn capture_schema_matches_closed_serialization_and_named_bounds() {
    let document = SubscriptionHighWater {
        format: "slingshot.agent/1".into(),
        transport_contract_digest: AuthorAgentTransportContract::embedded_digest(),
        daemon_subscription_identifier: "sub".into(),
        agent_event_store_generation: 7,
        high_water_cursor: "capture".into(),
    };
    let value = serde_json::to_value(&document).unwrap();
    let schema: serde_json::Value = serde_json::from_str(SCHEMA).unwrap();
    let mut required: Vec<_> =
        schema["required"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
    required.sort_unstable();
    assert_eq!(required, value.as_object().unwrap().keys().map(String::as_str).collect::<Vec<_>>());
    assert_eq!(
        required,
        schema["properties"].as_object().unwrap().keys().map(String::as_str).collect::<Vec<_>>()
    );
    assert_eq!(schema["additionalProperties"], false);
    assert_eq!(serde_json::from_value::<SubscriptionHighWater>(value.clone()).unwrap(), document);
    for key in &required {
        let mut missing = value.clone();
        missing.as_object_mut().unwrap().remove(*key);
        assert!(serde_json::from_value::<SubscriptionHighWater>(missing).is_err());
    }
    let mut surplus = value;
    surplus["command_provenance"] = serde_json::json!({});
    assert!(serde_json::from_value::<SubscriptionHighWater>(surplus).is_err());
    let contract = AuthorAgentTransportContract::embedded();
    assert_eq!(
        schema["properties"]["daemon_subscription_identifier"]["maxLength"],
        contract.limit("maximum_daemon_subscription_identifier_bytes")
    );
    assert_eq!(
        schema["properties"]["high_water_cursor"]["maxLength"],
        contract.limit("maximum_agent_operation_identifier_bytes")
    );
    assert_eq!(format!("{document:?}"), "SubscriptionHighWater([redacted])");
}
