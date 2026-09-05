//! Assertions for the common selected-author finite response gate.

use http::{HeaderValue, Response, Version};
use slingshot_agent_connection::selected_author_exchange::{
    CollectedFiniteResponse, SelectedAuthorExchangeRefusal, validate_collected_finite_response,
};

/// Supplies the observations a production finite-response reader must prove.
fn collected(response: Response<Vec<u8>>) -> CollectedFiniteResponse {
    CollectedFiniteResponse {
        response,
        framing_ambiguous: false,
        trailer_section_present: false,
        trailing_bytes: false,
    }
}

/// A normal identity-coded response passes unchanged.
#[test]
fn accepts_one_bounded_http11_identity_response() {
    let response = Response::builder()
        .version(Version::HTTP_11)
        .status(200)
        .header("content-encoding", "identity")
        .header("content-type", "application/json")
        .body(b"{}".to_vec())
        .expect("the test response builds");
    let accepted = validate_collected_finite_response(collected(response))
        .expect("the response is acceptable");
    assert_eq!(accepted.status, 200);
    assert_eq!(accepted.body, b"{}");
    assert_eq!(accepted.content_type.as_deref(), Some("application/json"));
    assert_eq!(accepted.retry_after, None);
}

#[test]
fn rejects_nonfinal_redirect_and_missing_media_even_without_network_reader() {
    for status in [100, 101, 103, 301, 302, 307, 600] {
        let response = Response::builder()
            .status(status)
            .header("content-type", "application/json")
            .body(Vec::new())
            .unwrap();
        assert_eq!(
            validate_collected_finite_response(collected(response)),
            Err(SelectedAuthorExchangeRefusal::Status)
        );
    }
    assert_eq!(
        validate_collected_finite_response(collected(Response::new(Vec::new()))),
        Err(SelectedAuthorExchangeRefusal::MissingContentType)
    );
}

#[test]
fn response_debug_does_not_expose_private_body_or_remote_metadata() {
    let response = Response::builder()
        .header("content-type", "private-sentinel")
        .body(b"private-sentinel".to_vec())
        .unwrap();
    let collected = collected(response);
    assert!(!format!("{collected:?}").contains("private-sentinel"));
    let accepted = validate_collected_finite_response(collected).unwrap();
    assert!(!format!("{accepted:?}").contains("private-sentinel"));
}

/// A repeated singleton response field is never interpreted as first or last.
#[test]
fn rejects_a_duplicate_content_coding_before_body_use() {
    let mut response = Response::builder()
        .version(Version::HTTP_2)
        .status(200)
        .body(Vec::new())
        .expect("the test response builds");
    response.headers_mut().append("content-encoding", HeaderValue::from_static("identity"));
    response.headers_mut().append("content-encoding", HeaderValue::from_static("gzip"));
    assert_eq!(
        validate_collected_finite_response(collected(response)),
        Err(SelectedAuthorExchangeRefusal::DuplicateSingletonHeader { name: "content-encoding" })
    );
}

/// A redirect is refused even when it names the same author origin.
#[test]
fn rejects_a_redirect_before_route_specific_decoding() {
    let response = Response::builder()
        .version(Version::HTTP_11)
        .status(302)
        .header("location", "https://author.example.test/same-origin")
        .body(Vec::new())
        .expect("the test response builds");
    assert!(validate_collected_finite_response(collected(response)).is_err());
}

/// Fields with a route-specific meaning cannot be silently selected from two
/// conflicting wire values.
#[test]
fn retains_singleton_route_metadata_and_refuses_ambiguous_duplicates() {
    let response = Response::builder()
        .version(Version::HTTP_2)
        .status(503)
        .header("content-type", "application/json")
        .header("retry-after", "5")
        .body(Vec::new())
        .expect("the test response builds");
    let accepted = validate_collected_finite_response(collected(response))
        .expect("the metadata is unambiguous");
    assert_eq!(accepted.content_type.as_deref(), Some("application/json"));
    assert_eq!(accepted.retry_after.as_deref(), Some("5"));

    let mut duplicate = Response::builder()
        .version(Version::HTTP_2)
        .status(200)
        .body(Vec::new())
        .expect("the test response builds");
    duplicate.headers_mut().append("content-type", HeaderValue::from_static("application/json"));
    duplicate.headers_mut().append("content-type", HeaderValue::from_static("text/plain"));
    assert_eq!(
        validate_collected_finite_response(collected(duplicate)),
        Err(SelectedAuthorExchangeRefusal::DuplicateSingletonHeader { name: "content-type" })
    );
}

/// A caller cannot turn an unproved finite message into a route response by
/// omitting the transport's framing observations.
#[test]
fn rejects_actual_trailers_ambiguous_framing_and_trailing_bytes() {
    for response in [
        CollectedFiniteResponse {
            response: Response::new(Vec::new()),
            framing_ambiguous: true,
            trailer_section_present: false,
            trailing_bytes: false,
        },
        CollectedFiniteResponse {
            response: Response::new(Vec::new()),
            framing_ambiguous: false,
            trailer_section_present: true,
            trailing_bytes: false,
        },
        CollectedFiniteResponse {
            response: Response::new(Vec::new()),
            framing_ambiguous: false,
            trailer_section_present: false,
            trailing_bytes: true,
        },
    ] {
        assert!(validate_collected_finite_response(response).is_err());
    }
}
