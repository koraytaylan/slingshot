//! Presenting a credential once, refreshing at most once, and keeping nothing.
//!
//! The refresh bound is the property worth testing. A token rejected right
//! after being obtained is not a token that will work on the third attempt:
//! something else is wrong, and retrying turns one failed request into a burst
//! against a system already refusing.

use slingshot_agent_connection::request_authentication::{
    AttemptOutcome, AuthenticatedRequest, AuthenticationFailure, MAXIMUM_REFRESHES_PER_REQUEST,
    ProviderKind, Retry, authorization_header,
};

/// A credential value, of the shape a real one has.
const ENCODED_CREDENTIAL: &str = "eyJhbGciOiJSUzI1NiJ9.not-a-real-token.c2ln";

#[test]
fn an_accepted_credential_ends_the_request() {
    for kind in [ProviderKind::Basic, ProviderKind::Bearer] {
        let mut request = AuthenticatedRequest::using(kind);
        assert_eq!(
            request.observe(AttemptOutcome::Accepted).expect("an accepted request"),
            Retry::Done
        );
        assert_eq!(request.refreshes(), 0, "and nothing was refreshed on the way");
    }
}

#[test]
fn only_a_token_has_anything_to_refresh() {
    assert!(!ProviderKind::Basic.is_refreshable());
    assert!(ProviderKind::Bearer.is_refreshable());

    let mut basic = AuthenticatedRequest::using(ProviderKind::Basic);
    assert_eq!(
        basic.observe(AttemptOutcome::CredentialRejected),
        Err(AuthenticationFailure::RejectedAndNotRefreshable { kind: ProviderKind::Basic }),
        "asking the snapshot again produces the same credential the author just rejected"
    );
    assert_eq!(basic.refreshes(), 0);
}

#[test]
fn a_token_refreshes_once_and_then_stops() {
    let mut request = AuthenticatedRequest::using(ProviderKind::Bearer);
    assert_eq!(
        request.observe(AttemptOutcome::CredentialRejected).expect("a first rejection"),
        Retry::AfterRefreshing,
        "an expired token is exactly what a refresh is for"
    );
    assert_eq!(request.refreshes(), MAXIMUM_REFRESHES_PER_REQUEST);

    assert_eq!(
        request.observe(AttemptOutcome::CredentialRejected),
        Err(AuthenticationFailure::RejectedAfterRefresh { kind: ProviderKind::Bearer }),
        "a freshly obtained token being rejected means something other than expiry is wrong"
    );
    assert_eq!(
        request.refreshes(),
        MAXIMUM_REFRESHES_PER_REQUEST,
        "and the refusal did not spend another refresh"
    );
}

#[test]
fn a_refreshed_token_that_works_ends_the_request() {
    let mut request = AuthenticatedRequest::using(ProviderKind::Bearer);
    request.observe(AttemptOutcome::CredentialRejected).expect("a first rejection");
    assert_eq!(
        request.observe(AttemptOutcome::Accepted).expect("the fresh token works"),
        Retry::Done
    );
}

#[test]
fn no_failure_this_module_produces_can_carry_a_credential() {
    let failures = [
        AuthenticationFailure::ProviderUnavailable { kind: ProviderKind::Bearer },
        AuthenticationFailure::RejectedAndNotRefreshable { kind: ProviderKind::Basic },
        AuthenticationFailure::RejectedAfterRefresh { kind: ProviderKind::Bearer },
    ];
    for failure in failures {
        let rendered = format!("{failure}{failure:?}");
        assert!(
            !rendered.contains(ENCODED_CREDENTIAL) && !rendered.contains("eyJ"),
            "a diagnostic carrying a credential would put it exactly where diagnostics go: \
             {rendered}"
        );
        assert!(
            rendered.contains("Basic") || rendered.contains("Bearer"),
            "while still naming which provider, which is what an operator needs"
        );
    }
}

#[test]
fn a_header_is_written_from_an_already_encoded_credential() {
    assert_eq!(
        authorization_header(ProviderKind::Bearer, ENCODED_CREDENTIAL),
        format!("Bearer {ENCODED_CREDENTIAL}"),
        "the scheme this kind presents, and the value it was handed"
    );
    assert_eq!(
        authorization_header(ProviderKind::Basic, "dXNlcjpwYXNzd29yZA=="),
        "Basic dXNlcjpwYXNzd29yZA==",
        "and nothing here ever holds a user name or a password in a form it could render"
    );
}
