//! Probe for the secret-buffers capability.
//!
//! Requires a wrapper whose rendering redacts the value, whose contents are
//! reachable only through an explicit call, and which reads from a serialized
//! document without leaking the value into a diagnostic.

use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
struct CredentialDocument {
    client_secret: SecretString,
}

#[test]
fn a_secret_redacts_its_rendering_and_exposes_only_on_request() {
    let secret = SecretString::from("p-8x7-not-a-real-secret");
    assert!(!format!("{secret:?}").contains("not-a-real-secret"), "{secret:?}");
    assert_eq!(secret.expose_secret(), "p-8x7-not-a-real-secret");

    let document: CredentialDocument =
        toml::from_str("client-secret = \"p-8x7-not-a-real-secret\"")
            .expect("the credential document reads");
    assert_eq!(document.client_secret.expose_secret(), "p-8x7-not-a-real-secret");
    let rendered = format!("{:?}", document.client_secret);
    assert!(!rendered.contains("not-a-real-secret"), "{rendered}");
}
