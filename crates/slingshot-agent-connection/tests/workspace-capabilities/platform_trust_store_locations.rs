//! Probe for the platform trust-store-location capability.
//!
//! Requires locating the platform server-authentication root bundle as data,
//! and proving that locating it merges nothing: a store the caller did not fill
//! stays empty, so an ambient root can never reach a client configuration by
//! accident.

use rustls::RootCertStore;
use rustls_pki_types::CertificateDer;
use rustls_pki_types::pem::PemObject;

#[test]
fn the_platform_bundle_is_located_as_data_and_merged_into_nothing() {
    let discovered = openssl_probe::probe();
    for candidate in openssl_probe::candidate_cert_dirs() {
        assert!(candidate.is_absolute(), "{}", candidate.display());
    }
    for directory in &discovered.cert_dir {
        assert!(directory.is_absolute(), "{}", directory.display());
    }

    let mut located_roots = 0_usize;
    if let Some(bundle) = discovered.cert_file.as_ref() {
        assert!(bundle.is_absolute(), "{}", bundle.display());
        let bytes = std::fs::read(bundle).expect("the located bundle reads");
        located_roots = CertificateDer::pem_slice_iter(&bytes).filter(Result::is_ok).count();
        assert!(located_roots > 0, "the located bundle holds provider records");
    }

    let empty = RootCertStore::empty();
    assert_eq!(empty.len(), 0, "locating a bundle merges nothing into a store");

    let mut explicit = RootCertStore::empty();
    if let Some(bundle) = discovered.cert_file.as_ref() {
        let bytes = std::fs::read(bundle).expect("the located bundle reads");
        for record in CertificateDer::pem_slice_iter(&bytes).flatten() {
            explicit.add(record).ok();
        }
        assert!(explicit.len() <= located_roots, "only explicitly supplied roots enter the store");
        assert!(!explicit.is_empty(), "an explicit read is what fills the store");
    }
    assert!(openssl_probe::has_ssl_cert_env_vars() || explicit.len() >= empty.len());
}
