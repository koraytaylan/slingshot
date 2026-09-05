//! Closed reset wire shape and named schema bounds.
use slingshot_agent_protocol::event_stream_reset::{EventStreamResetRequired, ResetReason, SCHEMA};
use slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract;

#[test]
fn reset_schema_and_serialization_require_explicit_nullable_request_cursor() {
    let document = EventStreamResetRequired {
        format: "slingshot.agent/1".into(),
        transport_contract_digest: AuthorAgentTransportContract::embedded_digest(),
        daemon_subscription_identifier: "subscription".into(),
        requested_agent_event_store_generation: 7,
        requested_last_event_identifier: None,
        agent_event_store_generation: 8,
        high_water_cursor: "captured".into(),
        reason: ResetReason::GenerationChanged,
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
    assert!(value["requested_last_event_identifier"].is_null());
    assert_eq!(
        serde_json::from_value::<EventStreamResetRequired>(value.clone()).unwrap(),
        document
    );
    let mut missing = value.clone();
    missing.as_object_mut().unwrap().remove("requested_last_event_identifier");
    assert!(serde_json::from_value::<EventStreamResetRequired>(missing).is_err());
    let mut surplus = value;
    surplus["command_provenance"] = serde_json::json!({});
    assert!(serde_json::from_value::<EventStreamResetRequired>(surplus).is_err());
    assert_eq!(schema["additionalProperties"], false);
    let contract = AuthorAgentTransportContract::embedded();
    assert_eq!(
        schema["properties"]["daemon_subscription_identifier"]["maxLength"],
        contract.limit("maximum_daemon_subscription_identifier_bytes")
    );
    assert_eq!(
        schema["properties"]["high_water_cursor"]["maxLength"],
        contract.limit("maximum_agent_operation_identifier_bytes")
    );
    assert_eq!(format!("{document:?}"), "EventStreamResetRequired([redacted])");
}
