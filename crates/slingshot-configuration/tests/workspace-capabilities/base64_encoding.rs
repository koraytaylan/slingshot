//! Probe for the Base64 capability.
//!
//! Requires both the standard and the URL-safe alphabets, exact padding, and
//! refusal of an invalid character, because signed assertions and credential
//! material use both alphabets.

use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};

#[test]
fn both_alphabets_round_trip_and_refuse_invalid_input() {
    let bytes = [0xfb_u8, 0xff, 0xbe];
    let standard = STANDARD.encode(bytes);
    assert_eq!(standard, "+/++");
    assert_eq!(STANDARD.decode(&standard).expect("the standard text decodes"), bytes);

    let url_safe = URL_SAFE_NO_PAD.encode(bytes);
    assert_eq!(url_safe, "-_--");
    assert_eq!(URL_SAFE_NO_PAD.decode(&url_safe).expect("the safe text decodes"), bytes);

    assert_eq!(STANDARD.encode([0_u8]), "AA==", "the standard alphabet pads");
    assert!(STANDARD.decode("****").is_err(), "an invalid character must be refused");
    assert!(URL_SAFE_NO_PAD.decode("AA==").is_err(), "unexpected padding must be refused");
}
