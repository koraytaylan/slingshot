//! One process verifies the production finite gate without sharing counters with other tests.

use http::{Response, StatusCode, Version};
use slingshot_agent_connection::selected_author_exchange::{
    CollectedFiniteResponse, validate_collected_finite_response,
};
use slingshot_agent_connection::submission_timing_observation::{Snapshot, snapshot};
use slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract;

const TIMINGS: &str = "admission;dur=3,execution;dur=17,persistence;dur=5";
const EXPECTED_DURATIONS: [u64; 3] = [3, 17, 5];

fn collected() -> CollectedFiniteResponse {
    CollectedFiniteResponse {
        response: Response::builder()
            .status(StatusCode::ACCEPTED)
            .version(Version::HTTP_11)
            .header("content-type", "application/json")
            .header("server-timing", TIMINGS)
            .body(b"{}".to_vec())
            .unwrap(),
        framing_ambiguous: false,
        trailer_section_present: false,
        trailing_bytes: false,
    }
}

fn require_unobserved_response(collected: CollectedFiniteResponse) {
    let before = snapshot();
    assert!(validate_collected_finite_response(collected).is_err());
    assert_eq!(snapshot(), before);
}

fn require_unobserved_boundaries() {
    for boundary in [0, 1, 2] {
        let mut collected = collected();
        match boundary {
            0 => collected.framing_ambiguous = true,
            1 => collected.trailer_section_present = true,
            _ => collected.trailing_bytes = true,
        }
        require_unobserved_response(collected);
    }
}

fn require_unobserved_heads() {
    let mut missing_type = collected();
    missing_type.response.headers_mut().remove("content-type");
    let mut compressed = collected();
    compressed
        .response
        .headers_mut()
        .insert("content-encoding", http::HeaderValue::from_static("gzip"));
    let mut repeated_retry = collected();
    repeated_retry
        .response
        .headers_mut()
        .append("retry-after", http::HeaderValue::from_static("1"));
    repeated_retry
        .response
        .headers_mut()
        .append("retry-after", http::HeaderValue::from_static("1"));
    for collected in [missing_type, compressed, repeated_retry] {
        require_unobserved_response(collected);
    }
}

#[test]
fn the_production_gate_observes_only_fully_validated_status_202_responses() {
    require_unobserved_boundaries();
    require_unobserved_heads();
    let mut oversize = collected();
    let limit =
        AuthorAgentTransportContract::embedded().limit("maximum_finite_response_body_bytes");
    *oversize.response.body_mut() = vec![0; usize::try_from(limit).unwrap() + 1];
    require_unobserved_response(oversize);

    let before = snapshot();
    let mut other_status = collected();
    *other_status.response.status_mut() = StatusCode::OK;
    assert!(validate_collected_finite_response(other_status).is_ok());
    assert_eq!(snapshot(), before);

    let response = validate_collected_finite_response(collected()).unwrap();
    assert_eq!(response.status, StatusCode::ACCEPTED.as_u16());
    assert_eq!(response.body, b"{}");
    let parsed = snapshot();
    assert_eq!(
        parsed,
        Snapshot {
            parsed_headers: before.parsed_headers + 1,
            admission_milliseconds: before.admission_milliseconds + EXPECTED_DURATIONS[0],
            execution_milliseconds: before.execution_milliseconds + EXPECTED_DURATIONS[1],
            persistence_milliseconds: before.persistence_milliseconds + EXPECTED_DURATIONS[2],
            ..before
        }
    );

    let mut unusable = collected();
    unusable
        .response
        .headers_mut()
        .insert("server-timing", http::HeaderValue::from_static("unusable"));
    let response = validate_collected_finite_response(unusable).unwrap();
    assert_eq!(response.status, StatusCode::ACCEPTED.as_u16());
    assert_eq!(response.body, b"{}");
    let rejected = snapshot();
    assert_eq!(rejected, Snapshot { rejected_headers: parsed.rejected_headers + 1, ..parsed });

    let mut absent = collected();
    absent.response.headers_mut().remove("server-timing");
    assert!(validate_collected_finite_response(absent).is_ok());
    assert_eq!(snapshot(), Snapshot { absent_headers: rejected.absent_headers + 1, ..rejected });
}
