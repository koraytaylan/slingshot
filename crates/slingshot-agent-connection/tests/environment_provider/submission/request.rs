//! Exact request identity and authentication checks shared by submission peers.

/// Checks a complete request head without exposing credential values in diagnostics.
pub(super) fn matches(bytes: &[u8], expected: &str, authentication: &[u8]) -> bool {
    let Ok(request) = std::str::from_utf8(bytes) else {
        return false;
    };
    if !request.ends_with("\r\n\r\n") {
        return false;
    }
    let mut lines = request.split("\r\n");
    if lines.next() != Some(expected) {
        return false;
    }
    let authorizations: Vec<&[u8]> = lines
        .filter_map(|line| line.split_once(':'))
        .filter(|(name, _)| name.eq_ignore_ascii_case("authorization"))
        .map(|(_, value)| value.trim_matches([' ', '\t']).as_bytes())
        .collect();
    authorizations == [authentication]
}

#[test]
fn token_request_requires_exact_route_and_authentication() {
    let expected = "GET /aem/libs/granite/csrf/token.json HTTP/1.1";
    let valid = format!("{expected}\r\nAuthorization: Basic fixture\r\n\r\n");
    assert!(matches(valid.as_bytes(), expected, b"Basic fixture"));
    assert!(!matches(valid.as_bytes(), expected, b"Basic other"));
    assert!(!matches(valid.as_bytes(), "GET /other HTTP/1.1", b"Basic fixture"));
    assert!(!matches(format!("{expected}\r\n\r\n").as_bytes(), expected, b"Basic fixture"));
}
