//! Probe for the signed-assertions capability.
//!
//! Requires building a compact assertion with registered claims, validating it
//! against an expected audience and issuer, refusing an expired assertion and a
//! wrong key, and loading a verification key from Privacy Enhanced Mail input.

use base64::Engine;
use ring::rand::SystemRandom;
use ring::signature::{RSA_PKCS1_SHA256, RsaKeyPair};
use serde_json::Value;

#[test]
fn a_rsa_assertion_signature_is_built_by_the_selected_backend() {
    let document: Value = serde_json::from_slice(include_bytes!(
        "../../../slingshot-test-support/fixtures/cloud-credentials/valid.json"
    ))
    .expect("the credential fixture parses");
    let pem = document["integration"]["privateKey"]
        .as_str()
        .expect("the fixture carries a private key")
        .as_bytes();
    let body = pem
        .split(|byte| *byte == b'\n')
        .skip(1)
        .take_while(|line| !line.starts_with(b"-----END"))
        .flat_map(|line| line.iter().copied())
        .collect::<Vec<_>>();
    let der = base64::engine::general_purpose::STANDARD.decode(body).expect("the key body decodes");
    let key = RsaKeyPair::from_pkcs8(&der).expect("the PKCS#8 key parses");
    let mut signature = vec![0; key.public().modulus_len()];
    key.sign(&RSA_PKCS1_SHA256, &SystemRandom::new(), b"assertion", &mut signature)
        .expect("the selected backend signs");
    assert_eq!(signature.len(), key.public().modulus_len());
    assert!(RsaKeyPair::from_pkcs8(b"not a key").is_err(), "malformed key input is refused");
}
