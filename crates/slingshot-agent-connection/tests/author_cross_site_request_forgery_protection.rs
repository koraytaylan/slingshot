//! The token the author requires, implemented exactly and for the right reason.
//!
//! Cross-site request forgery is a browser problem, and a daemon has no ambient
//! credentials to be abused. This is implemented because the author refuses
//! requests without it - so the tests are about doing what the far side asks
//! exactly, with no partial credit and no place where a missing token is
//! worked around.

use slingshot_agent_connection::author_cross_site_request_forgery_protection::{
    CrossSiteRequestForgeryToken, DeploymentEra, PROTECTED_METHODS, TOKEN_HEADER, TOKEN_ROUTE,
    TokenFailure, header_for, requires_token,
};

/// The author these fixtures talk to.
const AUTHOR: &str = "https://author.example";

/// Another author, to prove a token does not travel.
const ANOTHER_AUTHOR: &str = "https://elsewhere.example";

/// The context prefix one deployment era sits behind.
const CONTEXT_PREFIX: &str = "/aem";

/// One instant, for a test that does not care which.
const NOW: u64 = 1_700_000_000_000;

/// How long after now the fixture token expires.
const TOKEN_LIFETIME: u64 = 300_000;

/// Returns a token this author issued.
fn token() -> CrossSiteRequestForgeryToken {
    CrossSiteRequestForgeryToken {
        expires_at_unix_milliseconds: NOW + TOKEN_LIFETIME,
        origin: AUTHOR.to_owned(),
        value: "a-token-value".to_owned(),
    }
}

#[test]
fn each_deployment_era_is_asked_where_it_actually_keeps_the_token() {
    assert_eq!(DeploymentEra::Cloud.token_route(), TOKEN_ROUTE);
    assert_eq!(
        DeploymentEra::ManagedServices { context_prefix: CONTEXT_PREFIX.to_owned() }.token_route(),
        format!("{CONTEXT_PREFIX}{TOKEN_ROUTE}"),
        "a client that guessed would fetch from a path that does not exist on one era and read \
         the refusal as something else"
    );
}

#[test]
fn only_a_request_that_changes_something_needs_a_token() {
    for method in PROTECTED_METHODS {
        assert!(requires_token(method), "{method} changes something");
    }
    for method in ["GET", "HEAD", "OPTIONS"] {
        assert!(
            !requires_token(method),
            "{method}: a token on a read is ceremony, because a forged read does nothing reading \
             normally would not"
        );
        assert_eq!(header_for(method, None, AUTHOR, NOW).expect("a read needs nothing"), None);
    }
}

#[test]
fn a_request_that_changes_something_without_a_token_is_refused_here() {
    assert_eq!(
        header_for("POST", None, AUTHOR, NOW),
        Err(TokenFailure::Absent),
        "sending it would mean the author has to decide, which is one more place the request \
         could be interpreted before being rejected"
    );
}

#[test]
fn a_token_is_presented_in_the_header_the_author_reads() {
    let held = token();
    let (name, value) = header_for("POST", Some(&held), AUTHOR, NOW)
        .expect("a token this author issued")
        .expect("a request that changes something");
    assert_eq!(name, TOKEN_HEADER);
    assert_eq!(value, held.value);
}

#[test]
fn an_expired_token_is_refused_rather_than_sent_and_rejected() {
    let held = token();
    held.present_to(AUTHOR, NOW + TOKEN_LIFETIME - 1).expect("one millisecond before it expires");
    assert_eq!(
        held.present_to(AUTHOR, NOW + TOKEN_LIFETIME),
        Err(TokenFailure::Expired { expired_at: NOW + TOKEN_LIFETIME }),
        "and exactly at its expiry"
    );
}

#[test]
fn a_token_fetched_from_one_author_is_never_presented_to_another() {
    let held = token();
    assert_eq!(
        held.present_to(ANOTHER_AUTHOR, NOW),
        Err(TokenFailure::AnotherOrigin),
        "presenting it there would leak it to somewhere that never issued it"
    );
    assert_eq!(
        header_for("POST", Some(&held), ANOTHER_AUTHOR, NOW),
        Err(TokenFailure::AnotherOrigin)
    );
}

#[test]
fn no_failure_this_module_produces_carries_the_token_value() {
    let held = token();
    for failure in [
        TokenFailure::Absent,
        TokenFailure::Expired { expired_at: NOW },
        TokenFailure::AnotherOrigin,
    ] {
        let rendered = format!("{failure}{failure:?}");
        assert!(!rendered.contains(&held.value), "a token is presented and not logged: {rendered}");
    }
}
