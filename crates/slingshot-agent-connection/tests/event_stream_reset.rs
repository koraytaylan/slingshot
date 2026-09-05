//! Reset evidence requires closed request echoes and consistent status semantics.
use slingshot_agent_connection::event_stream_reset::{
    ResetRequest, ResetRoute, decode_event_reset,
};
use slingshot_agent_connection::selected_author_exchange::{
    CollectedFiniteResponse, SelectedAuthorFiniteResponse, validate_collected_finite_response,
};
use slingshot_agent_protocol::event_stream_reset::EventStreamResetRequired;
use slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract;

fn document(changed: bool) -> serde_json::Value {
    serde_json::json!({
        "format":"slingshot.agent/1", "transport_contract_digest":AuthorAgentTransportContract::embedded_digest(),
        "daemon_subscription_identifier":"sub", "requested_agent_event_store_generation":7,
        "requested_last_event_identifier":"cursor-old", "agent_event_store_generation":if changed {8} else {7},
        "high_water_cursor":"cursor-new", "reason":if changed {"generation_changed"} else {"cursor_expired"},
    })
}
fn response(status: u16, body: Vec<u8>) -> SelectedAuthorFiniteResponse {
    validate_collected_finite_response(CollectedFiniteResponse {
        response: http::Response::builder()
            .status(status)
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
fn request() -> ResetRequest<'static> {
    ResetRequest {
        route: ResetRoute::Events,
        subscription: "sub",
        generation: 7,
        committed_cursor: Some("cursor-old"),
    }
}
#[test]
fn valid_resets_preserve_request_and_capture_without_installing_a_cursor() {
    for (status, changed) in [(409, true), (410, false)] {
        let response = response(status, serde_json::to_vec(&document(changed)).unwrap());
        let reset = decode_event_reset(&response, request()).unwrap();
        assert_eq!(reset.subscription(), "sub");
        assert_eq!(reset.requested_generation(), 7);
        assert_eq!(reset.requested_cursor(), Some("cursor-old"));
        assert_eq!(reset.generation(), if changed { 8 } else { 7 });
        assert_eq!(reset.captured_cursor().as_text(), "cursor-new");
        assert_eq!(format!("{reset:?}"), "ValidatedEventReset([redacted])");
        assert_eq!(
            reset.reason(),
            if changed {
                slingshot_agent_connection::event_stream_reconnection::ResetReason::GenerationChanged
            } else {
                slingshot_agent_connection::event_stream_reconnection::ResetReason::CursorExpired
            }
        );
    }
}
#[test]
fn every_missing_surplus_duplicate_or_mismatched_echo_is_refused() {
    let original = document(true);
    for name in original.as_object().unwrap().keys() {
        let mut body = original.clone();
        body.as_object_mut().unwrap().remove(name);
        assert!(
            serde_json::from_value::<EventStreamResetRequired>(body.clone()).is_err(),
            "{name}"
        );
        assert!(
            decode_event_reset(&response(409, serde_json::to_vec(&body).unwrap()), request())
                .is_err()
        );
    }
    for (name, value) in [
        ("format", serde_json::json!("slingshot.agent/2")),
        ("transport_contract_digest", serde_json::json!("0".repeat(64))),
        ("daemon_subscription_identifier", serde_json::json!("other")),
        ("requested_agent_event_store_generation", serde_json::json!(6)),
        ("requested_last_event_identifier", serde_json::json!("other")),
        ("agent_event_store_generation", serde_json::json!(0)),
        ("high_water_cursor", serde_json::json!("bad\r\ncursor")),
        ("reason", serde_json::json!("unknown")),
        ("extra", serde_json::json!(true)),
    ] {
        let mut body = original.clone();
        body[name] = value;
        assert!(
            decode_event_reset(&response(409, serde_json::to_vec(&body).unwrap()), request())
                .is_err(),
            "{name}"
        );
    }
    let text = serde_json::to_string(&original).unwrap();
    let duplicate = text.replacen('{', "{\"reason\":\"generation_changed\",", 1);
    assert!(decode_event_reset(&response(409, duplicate.into_bytes()), request()).is_err());
}
#[test]
fn status_reason_generation_and_cursor_relationships_are_closed() {
    for status in [200, 401, 403, 404, 409, 410, 500] {
        for changed in [false, true] {
            let accepted = status == if changed { 409 } else { 410 };
            assert_eq!(
                decode_event_reset(
                    &response(status, serde_json::to_vec(&document(changed)).unwrap()),
                    request()
                )
                .is_ok(),
                accepted
            );
        }
    }
    let mut expired = document(false);
    expired["requested_last_event_identifier"] = serde_json::Value::Null;
    let mut context = request();
    context.committed_cursor = None;
    assert!(
        decode_event_reset(&response(410, serde_json::to_vec(&expired).unwrap()), context).is_err()
    );
    let mut changed = document(true);
    changed["requested_last_event_identifier"] = serde_json::Value::Null;
    for route in [ResetRoute::Events, ResetRoute::HighWater] {
        let context =
            ResetRequest { route, subscription: "sub", generation: 7, committed_cursor: None };
        assert!(
            decode_event_reset(&response(409, serde_json::to_vec(&changed).unwrap()), context)
                .is_ok()
        );
    }
    let mut context = request();
    context.route = ResetRoute::HighWater;
    assert!(
        decode_event_reset(&response(410, serde_json::to_vec(&document(false)).unwrap()), context)
            .is_err()
    );
}
#[test]
fn malformed_private_cursors_media_and_documents_do_not_become_reset_evidence() {
    for cursor in [
        "".to_owned(),
        "x".repeat(97),
        "é".repeat(49),
        " leading".to_owned(),
        "trailing ".to_owned(),
        "bad\0value".to_owned(),
    ] {
        let mut body = document(true);
        body["high_water_cursor"] = serde_json::json!(cursor);
        assert!(
            decode_event_reset(&response(409, serde_json::to_vec(&body).unwrap()), request())
                .is_err()
        );
    }
    for bytes in [b"{}".to_vec(), b"null".to_vec(), b"[]".to_vec(), b"{".to_vec()] {
        assert!(decode_event_reset(&response(409, bytes), request()).is_err());
    }
    let mut value = response(409, serde_json::to_vec(&document(true)).unwrap());
    value.content_type = Some("text/html".into());
    assert!(decode_event_reset(&value, request()).is_err());
    value.content_type = Some("application/json".into());
    value.head.location = Some("/elsewhere".into());
    assert!(decode_event_reset(&value, request()).is_err());
    value.head.location = None;
    value.head.content_coding = Some("gzip".into());
    assert!(decode_event_reset(&value, request()).is_err());
    value.head.content_coding = None;
    value.head.protocol_version = "HTTP/1.0".into();
    assert!(decode_event_reset(&value, request()).is_err());
    let mut body = document(true);
    body["high_water_cursor"] = serde_json::json!("x".repeat(96));
    assert!(
        decode_event_reset(&response(409, serde_json::to_vec(&body).unwrap()), request()).is_ok()
    );
}

#[test]
fn reset_recovery_checks_current_context_and_never_installs_the_captured_position() {
    use slingshot_agent_connection::event_stream_reconnection::{
        EventStreamReconnection, ResetReason, StreamHealth,
    };
    use slingshot_agent_connection::server_sent_event_decoder::EventStreamCursor;
    let reset =
        decode_event_reset(&response(409, serde_json::to_vec(&document(true)).unwrap()), request())
            .unwrap();
    for defect in ["", "subscription", "generation", "cursor", "conflict"] {
        let mut connection = EventStreamReconnection::opening(
            if defect == "subscription" { "other" } else { "sub" },
            if defect == "generation" { 6 } else { 7 },
        );
        let cursor = EventStreamCursor::new(
            if defect == "cursor" { "cursor-later" } else { "cursor-old" },
            96,
        )
        .unwrap();
        connection.commit_cursor(&cursor, "digest");
        if defect == "conflict" {
            connection.commit_cursor(&cursor, "different");
        }
        let before = connection.clone();
        let result = connection.require_validated_reset(&reset);
        assert_eq!(result.is_ok(), defect.is_empty());
        if defect.is_empty() {
            assert_eq!(connection.health(), StreamHealth::Degraded);
            assert_eq!(connection.outstanding_reset(), Some(ResetReason::GenerationChanged));
            assert_eq!(connection.ledger().generation(), 7);
            assert_eq!(connection.last_event_identifier(), Some("cursor-old"));
            assert!(connection.route().is_err());
            let held = connection.clone();
            assert!(connection.require_validated_reset(&reset).is_err());
            assert_eq!(connection, held);
        } else {
            assert_eq!(connection, before);
        }
    }
}
