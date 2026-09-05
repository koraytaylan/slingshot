//! Captures are closed request-bound evidence, never cursor installation authority.
use slingshot_agent_connection::selected_author_exchange::{
    CollectedFiniteResponse, SelectedAuthorFiniteResponse, validate_collected_finite_response,
};
use slingshot_agent_connection::subscription_high_water::decode_high_water;
use slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract;

fn document() -> serde_json::Value {
    serde_json::json!({
        "format":"slingshot.agent/1",
        "transport_contract_digest":AuthorAgentTransportContract::embedded_digest(),
        "daemon_subscription_identifier":"sub",
        "agent_event_store_generation":7,
        "high_water_cursor":"cursor-new"
    })
}
fn response(body: Vec<u8>) -> SelectedAuthorFiniteResponse {
    validate_collected_finite_response(CollectedFiniteResponse {
        response: http::Response::builder()
            .status(200)
            .version(http::Version::HTTP_2)
            .header("content-type", "application/json")
            .body(body)
            .unwrap(),
        framing_ambiguous: false,
        trailer_section_present: false,
        trailing_bytes: false,
    })
    .unwrap()
}
#[test]
fn capture_preserves_independently_bound_context_and_redacts_debug() {
    let value = response(serde_json::to_vec(&document()).unwrap());
    let capture = decode_high_water(&value, "sub", 7).unwrap();
    assert_eq!(capture.subscription(), "sub");
    assert_eq!(capture.generation(), 7);
    assert_eq!(capture.cursor().as_text(), "cursor-new");
    assert_eq!(format!("{capture:?}"), "ValidatedHighWater([redacted])");
    assert!(decode_high_water(&value, "other", 7).is_err());
    assert!(decode_high_water(&value, "sub", 8).is_err());
    assert!(decode_high_water(&value, "sub", 0).is_err());
}
#[test]
fn closed_document_refuses_missing_duplicate_surplus_and_invalid_values() {
    for key in document().as_object().unwrap().keys() {
        let mut body = document();
        body.as_object_mut().unwrap().remove(key);
        assert!(
            decode_high_water(&response(serde_json::to_vec(&body).unwrap()), "sub", 7).is_err()
        );
    }
    for (key, value) in [
        ("format", serde_json::json!("other")),
        ("transport_contract_digest", serde_json::json!("0".repeat(64))),
        ("daemon_subscription_identifier", serde_json::json!("other")),
        ("agent_event_store_generation", serde_json::json!(8)),
        ("extra", serde_json::json!(true)),
    ] {
        let mut body = document();
        body[key] = value;
        assert!(
            decode_high_water(&response(serde_json::to_vec(&body).unwrap()), "sub", 7).is_err()
        );
    }
    let duplicate = serde_json::to_string(&document()).unwrap().replacen(
        '{',
        "{\"agent_event_store_generation\":7,",
        1,
    );
    assert!(decode_high_water(&response(duplicate.into_bytes()), "sub", 7).is_err());
    for cursor in [
        "".into(),
        "x".repeat(97),
        "é".repeat(49),
        " leading".into(),
        "trailing ".into(),
        "bad\r\ncursor".into(),
    ] {
        let mut body = document();
        body["high_water_cursor"] = serde_json::json!(cursor);
        assert!(
            decode_high_water(&response(serde_json::to_vec(&body).unwrap()), "sub", 7).is_err()
        );
    }
    for cursor in ["x".repeat(96), "é".repeat(48)] {
        let mut body = document();
        body["high_water_cursor"] = serde_json::json!(cursor);
        assert!(decode_high_water(&response(serde_json::to_vec(&body).unwrap()), "sub", 7).is_ok());
    }
}
#[test]
fn status_and_head_policy_cannot_be_replaced_by_matching_json() {
    let bytes = serde_json::to_vec(&document()).unwrap();
    for status in [204, 401, 403, 409, 410, 500] {
        let mut value = response(bytes.clone());
        value.status = status;
        assert!(decode_high_water(&value, "sub", 7).is_err());
    }
    let mut value = response(bytes);
    value.content_type = Some("text/html".into());
    assert!(decode_high_water(&value, "sub", 7).is_err());
    value.content_type = Some("application/json".into());
    value.head.location = Some("/elsewhere".into());
    assert!(decode_high_water(&value, "sub", 7).is_err());
    value.head.location = None;
    value.head.content_coding = Some("gzip".into());
    assert!(decode_high_water(&value, "sub", 7).is_err());
}
