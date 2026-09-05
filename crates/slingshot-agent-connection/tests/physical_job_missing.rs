//! Physical absence needs a complete route response and independent request echoes.
use slingshot_agent_connection::{
    physical_job_missing::decode_physical_job_missing,
    selected_author_exchange::{
        CollectedFiniteResponse, SelectedAuthorFiniteResponse, validate_collected_finite_response,
    },
};
use slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract;

fn document() -> serde_json::Value {
    serde_json::json!({"format":"slingshot.agent/1", "transport_contract_digest":AuthorAgentTransportContract::embedded_digest(),
        "kind":"missing", "agent_event_store_generation":8, "sling_job_identifier":"job /?é"})
}
fn response(body: Vec<u8>) -> SelectedAuthorFiniteResponse {
    validate_collected_finite_response(CollectedFiniteResponse {
        response: http::Response::builder()
            .status(404)
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
fn physical_absence_is_current_generation_evidence_for_one_exact_identifier() {
    let value = response(serde_json::to_vec(&document()).unwrap());
    let proof = decode_physical_job_missing(&value, "job /?é", 8).unwrap();
    assert_eq!(proof.generation(), 8);
    assert_eq!(proof.sling_job_identifier(), "job /?é");
    assert_eq!(format!("{proof:?}"), "ValidatedPhysicalJobMissing([redacted])");
    assert!(decode_physical_job_missing(&value, "other", 8).is_err());
    assert!(decode_physical_job_missing(&value, "job /?é", 7).is_err());
    assert!(decode_physical_job_missing(&value, "job /?é", 0).is_err());
}
#[test]
fn closed_shape_refuses_missing_duplicate_surplus_and_mismatched_members() {
    for key in document().as_object().unwrap().keys() {
        let mut body = document();
        body.as_object_mut().unwrap().remove(key);
        assert!(
            decode_physical_job_missing(
                &response(serde_json::to_vec(&body).unwrap()),
                "job /?é",
                8
            )
            .is_err()
        );
    }
    for (key, value) in [
        ("format", serde_json::json!("other")),
        ("kind", serde_json::json!("retired")),
        ("agent_event_store_generation", serde_json::json!(9)),
        ("sling_job_identifier", serde_json::json!("other")),
        ("transport_contract_digest", serde_json::json!("0".repeat(64))),
        ("provenance", serde_json::json!({})),
        ("agent_operation_identifier", serde_json::json!("invented")),
    ] {
        let mut body = document();
        body[key] = value;
        assert!(
            decode_physical_job_missing(
                &response(serde_json::to_vec(&body).unwrap()),
                "job /?é",
                8
            )
            .is_err()
        );
    }
    let duplicate =
        serde_json::to_string(&document()).unwrap().replacen('{', "{\"kind\":\"missing\",", 1);
    assert!(decode_physical_job_missing(&response(duplicate.into_bytes()), "job /?é", 8).is_err());
    for identifier in [String::new(), "x".repeat(1025), "é".repeat(513)] {
        let mut body = document();
        body["sling_job_identifier"] = serde_json::json!(identifier);
        assert!(
            decode_physical_job_missing(
                &response(serde_json::to_vec(&body).unwrap()),
                &identifier,
                8
            )
            .is_err()
        );
    }
    for identifier in ["x".repeat(1024), "é".repeat(512)] {
        let mut body = document();
        body["sling_job_identifier"] = serde_json::json!(identifier);
        assert!(
            decode_physical_job_missing(
                &response(serde_json::to_vec(&body).unwrap()),
                &identifier,
                8
            )
            .is_ok()
        );
    }
}
#[test]
fn status_head_and_complete_document_are_required() {
    for status in [200, 401, 403, 409, 410, 500] {
        let mut value = response(serde_json::to_vec(&document()).unwrap());
        value.status = status;
        assert!(decode_physical_job_missing(&value, "job /?é", 8).is_err());
    }
    let mut value = response(serde_json::to_vec(&document()).unwrap());
    value.content_type = Some("text/html".into());
    assert!(decode_physical_job_missing(&value, "job /?é", 8).is_err());
    value.content_type = Some("application/json".into());
    value.head.location = Some("/other".into());
    assert!(decode_physical_job_missing(&value, "job /?é", 8).is_err());
    value.head.location = None;
    value.head.content_coding = Some("gzip".into());
    assert!(decode_physical_job_missing(&value, "job /?é", 8).is_err());
    for body in [b"{}".to_vec(), b"null".to_vec(), b"{".to_vec()] {
        assert!(decode_physical_job_missing(&response(body), "job /?é", 8).is_err());
    }
}
