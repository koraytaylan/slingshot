//! Probe for the transport-layer-security capability.
//!
//! Requires client configurations built from explicitly supplied immutable root
//! stores with no ambient root discovery, and proof that a root supplied only to
//! the author route cannot authenticate a record on the identity-management
//! route.

use std::sync::Arc;

use rustls::client::WebPkiServerVerifier;
use rustls::client::danger::ServerCertVerifier;
use rustls::crypto::ring;
use rustls::{ClientConfig, RootCertStore};
use rustls_pki_types::{ServerName, UnixTime};

use crate::material;

/// Number of roots each explicitly supplied store holds.
const IDENTITY_MANAGEMENT_ROOT_COUNT: usize = 1;

/// Number of roots the author store holds once its extra root is supplied.
const AUTHOR_ROOT_COUNT: usize = 2;

/// Builds a store holding exactly the named roots and nothing else.
fn explicit_store(names: &[&str]) -> Arc<RootCertStore> {
    let mut store = RootCertStore::empty();
    for name in names {
        store.add(material::certificate(name)).expect("the root is accepted");
    }
    Arc::new(store)
}

/// Builds a server verifier over exactly one explicitly supplied store.
fn verifier(store: Arc<RootCertStore>) -> Arc<WebPkiServerVerifier> {
    WebPkiServerVerifier::builder_with_provider(store, Arc::new(ring::default_provider()))
        .build()
        .expect("the verifier builds over the supplied store")
}

#[test]
fn each_route_authenticates_only_the_roots_it_was_given() {
    let identity_management = explicit_store(&["identity-management-root"]);
    let author = explicit_store(&["identity-management-root", "author-root"]);
    assert_eq!(
        identity_management.len(),
        IDENTITY_MANAGEMENT_ROOT_COUNT,
        "no ambient root is merged"
    );
    assert_eq!(author.len(), AUTHOR_ROOT_COUNT, "the author route adds exactly one root");

    let author_record = material::certificate("author-leaf");
    let name = ServerName::try_from("author.example.invalid").expect("the host name is valid");
    let now = UnixTime::now();

    let refused = verifier(Arc::clone(&identity_management)).verify_server_cert(
        &author_record,
        &[],
        &name,
        &[],
        now,
    );
    assert!(refused.is_err(), "the author-only root must not authenticate identity management");

    let accepted =
        verifier(Arc::clone(&author)).verify_server_cert(&author_record, &[], &name, &[], now);
    assert!(accepted.is_ok(), "the author route authenticates its own record: {accepted:?}");

    let wrong_name =
        ServerName::try_from("identity.example.invalid").expect("the host name is valid");
    let mismatched =
        verifier(author).verify_server_cert(&author_record, &[], &wrong_name, &[], now);
    assert!(mismatched.is_err(), "a record must not authenticate another host");

    let configuration = ClientConfig::builder_with_provider(Arc::new(ring::default_provider()))
        .with_safe_default_protocol_versions()
        .expect("the protocol versions are supported")
        .with_root_certificates(identity_management)
        .with_no_client_auth();
    assert!(configuration.alpn_protocols.is_empty(), "no protocol is negotiated by default");
}
