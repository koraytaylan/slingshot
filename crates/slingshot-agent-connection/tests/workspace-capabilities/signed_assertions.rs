//! Probe for the signed-assertions capability.
//!
//! Requires building a compact assertion with registered claims, validating it
//! against an expected audience and issuer, refusing an expired assertion and a
//! wrong key, and loading a verification key from Privacy Enhanced Mail input.

use std::time::{SystemTime, UNIX_EPOCH};

use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};

use crate::material;

/// Seconds an accepted assertion stays valid.
const ASSERTION_LIFETIME_SECONDS: u64 = 300;

/// Seconds an already-expired assertion was valid for.
const EXPIRED_OFFSET_SECONDS: u64 = 3_600;

/// Claims a Slingshot access-token exchange sends.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
struct ExchangeClaims {
    iss: String,
    sub: String,
    aud: String,
    exp: u64,
}

/// Returns the current time in seconds since the epoch.
fn now_seconds() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).expect("the clock is after the epoch").as_secs()
}

#[test]
fn a_compact_assertion_is_built_validated_and_refused_when_stale() {
    let key = b"a probe signing key that is not a credential";
    let encoding = EncodingKey::from_secret(key);
    let decoding = DecodingKey::from_secret(key);

    let claims = ExchangeClaims {
        iss: "organization@AdobeOrg".to_owned(),
        sub: "technical-account@techacct.adobe.com".to_owned(),
        aud: "https://identity.example.invalid/c/client".to_owned(),
        exp: now_seconds() + ASSERTION_LIFETIME_SECONDS,
    };
    let assertion =
        encode(&Header::new(Algorithm::HS256), &claims, &encoding).expect("the assertion signs");
    assert_eq!(assertion.split('.').count(), 3, "the assertion is compact");

    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_audience(&[claims.aud.as_str()]);
    validation.set_issuer(&[claims.iss.as_str()]);
    let decoded = decode::<ExchangeClaims>(&assertion, &decoding, &validation)
        .expect("the assertion validates");
    assert_eq!(decoded.claims, claims);
    assert_eq!(decoded.header.alg, Algorithm::HS256);

    let other = DecodingKey::from_secret(b"another probe signing key entirely");
    assert!(
        decode::<ExchangeClaims>(&assertion, &other, &validation).is_err(),
        "a wrong key is refused"
    );

    let stale = ExchangeClaims { exp: now_seconds() - EXPIRED_OFFSET_SECONDS, ..claims };
    let expired =
        encode(&Header::new(Algorithm::HS256), &stale, &encoding).expect("the assertion signs");
    assert!(
        decode::<ExchangeClaims>(&expired, &decoding, &validation).is_err(),
        "an expired assertion is refused"
    );

    let public = std::fs::read(material::certificate_path("author-root-public-key.pem"))
        .expect("the verification key reads");
    DecodingKey::from_rsa_pem(&public)
        .expect("a signature key loads from Privacy Enhanced Mail input");
    assert!(DecodingKey::from_rsa_pem(b"not a key").is_err(), "malformed key input is refused");
}
