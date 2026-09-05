//! Physical absence schema matches its closed serialized wire shape.
use slingshot_agent_protocol::physical_job_missing::{PhysicalJobMissing, SCHEMA};
#[test]
fn absence_schema_does_not_invent_logical_or_command_identity() {
    let document = PhysicalJobMissing { format: "slingshot.agent/1".into(), kind: "missing".into(),
        agent_event_store_generation: 8, sling_job_identifier: "physical".into(),
        transport_contract_digest: slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract::embedded_digest() };
    let schema: serde_json::Value = serde_json::from_str(SCHEMA).unwrap();
    let value = serde_json::to_value(&document).unwrap();
    let required: Vec<_> =
        schema["required"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
    assert_eq!(required, value.as_object().unwrap().keys().map(String::as_str).collect::<Vec<_>>());
    assert_eq!(
        required,
        schema["properties"].as_object().unwrap().keys().map(String::as_str).collect::<Vec<_>>()
    );
    assert_eq!(schema["additionalProperties"], false);
    assert_eq!(
        schema["properties"]["sling_job_identifier"]["maxLength"],
        slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract::embedded()
            .limit("maximum_sling_job_identifier_bytes")
    );
    assert_eq!(serde_json::from_value::<PhysicalJobMissing>(value).unwrap(), document);
    assert_eq!(format!("{document:?}"), "PhysicalJobMissing([redacted])");
}
