//! Probe for the protocol-types capability.
//!
//! Requires building a request and a response head, reading repeated header
//! fields in order, refusing a header value that would split a message, and
//! keeping the exact status and version of a head.

use http::header::{ACCEPT, HeaderValue, SET_COOKIE};
use http::{Method, Request, Response, StatusCode, Version};

#[test]
fn heads_are_built_read_and_refuse_a_splitting_value() {
    let request = Request::builder()
        .method(Method::POST)
        .uri("https://author.example.invalid/bin/querybuilder.json")
        .header(ACCEPT, "application/json")
        .body(())
        .expect("the request head builds");
    assert_eq!(request.method(), Method::POST);
    assert_eq!(request.uri().host(), Some("author.example.invalid"));
    assert_eq!(
        request.headers().get(ACCEPT).and_then(|value| value.to_str().ok()),
        Some("application/json")
    );

    let mut response = Response::builder()
        .status(StatusCode::EARLY_HINTS)
        .version(Version::HTTP_11)
        .body(())
        .expect("the response head builds");
    assert!(response.status().is_informational());
    *response.status_mut() = StatusCode::OK;
    response.headers_mut().append(SET_COOKIE, HeaderValue::from_static("first=1"));
    response.headers_mut().append(SET_COOKIE, HeaderValue::from_static("second=2"));
    let repeated: Vec<&str> = response
        .headers()
        .get_all(SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .collect();
    assert_eq!(repeated, vec!["first=1", "second=2"]);

    assert!(
        HeaderValue::from_str("value\r\nInjected: yes").is_err(),
        "a splitting value is refused"
    );
    assert!(HeaderValue::from_str("value\0").is_err(), "a control byte is refused");
}
