//! Assertions for the optional author trust extension.
//!
//! The certificates here are real, generated once and committed, because a
//! parser proved only against bytes it also produced proves very little.
//!
//! Two properties matter. A source that would have to be interpreted
//! charitably - a leaf certificate, an authority whose stated purpose is
//! something else, a file that also carries a private key, surplus text - is
//! refused rather than partly used. And no refusal carries a byte of the source:
//! not a subject, not a certificate, not a parser message, because the file may
//! sit beside credentials and a diagnostic is not a place to read one from.

use std::path::PathBuf;

use slingshot_configuration::additional_certificate_authority::AdditionalAuthorCertificates;
use slingshot_configuration::profile_loader::{DiagnosticSourceClass, DiagnosticStage};
use slingshot_domain::profile_authentication_contract::{
    ConfigurationFailureCode, ProfileAuthenticationContract,
};

/// Directory holding the committed certificates.
const CERTIFICATE_FIXTURES: &str =
    "../slingshot-test-support/fixtures/additional-certificate-authority";

/// Returns one committed certificate source.
fn source(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(CERTIFICATE_FIXTURES).join(name);
    std::fs::read(&path)
        .unwrap_or_else(|failure| panic!("{} could not be read: {failure}", path.display()))
}

#[test]
fn a_source_of_authorities_is_accepted_in_the_order_it_lists_them() {
    let one = AdditionalAuthorCertificates::parse(&source("one-authority.pem"))
        .expect("one authority is accepted");
    assert_eq!(one.certificates().len(), 1);

    let two = AdditionalAuthorCertificates::parse(&source("two-authorities.pem"))
        .expect("two authorities are accepted");
    assert_eq!(two.certificates().len(), 2);
    assert_eq!(two.certificates()[0], one.certificates()[0], "the order changed");
    assert_ne!(two.certificates()[0], two.certificates()[1]);
}

#[test]
fn a_source_carrying_a_private_key_is_refused_as_a_private_key() {
    let diagnostic = AdditionalAuthorCertificates::parse(&source("with-private-key.pem"))
        .expect_err("a private key is refused");
    assert_eq!(diagnostic.code, ConfigurationFailureCode::AdditionalCertificateAuthorityPrivateKey);
    assert_eq!(diagnostic.source_class, DiagnosticSourceClass::AdditionalCertificateAuthority);
    assert_eq!(diagnostic.stage, DiagnosticStage::DocumentSemantics);
}

#[test]
fn every_source_that_would_have_to_be_guessed_at_is_refused() {
    for name in [
        "end-entity.pem",
        "other-purpose.pem",
        "duplicate-authority.pem",
        "malformed.pem",
        "empty.pem",
        "surplus-text.pem",
    ] {
        let diagnostic = AdditionalAuthorCertificates::parse(&source(name))
            .map_or_else(|diagnostic| diagnostic, |_| panic!("{name} was accepted"));
        assert_eq!(
            diagnostic.code,
            ConfigurationFailureCode::AdditionalCertificateAuthorityInvalid,
            "{name}"
        );
    }
}

#[test]
fn a_source_larger_than_its_bound_is_refused_before_it_is_parsed() {
    let bound = ProfileAuthenticationContract::embedded()
        .limits
        .maximum_additional_certificate_authority_document_bytes;
    let oversized = vec![b'-'; usize::try_from(bound).expect("the bound fits") + 1];
    let diagnostic =
        AdditionalAuthorCertificates::parse(&oversized).expect_err("an oversized source refuses");
    assert_eq!(
        diagnostic.code,
        ConfigurationFailureCode::AdditionalCertificateAuthorityLimitExceeded
    );
}

#[test]
fn no_refusal_carries_a_byte_of_the_source() {
    for name in ["end-entity.pem", "with-private-key.pem", "malformed.pem", "surplus-text.pem"] {
        let bytes = source(name);
        let text = String::from_utf8_lossy(&bytes);
        let diagnostic =
            AdditionalAuthorCertificates::parse(&bytes).expect_err("the source is refused");
        let rendered = format!("{diagnostic:?}");
        for sentinel in ["Slingshot Test Authority", "author.example.com", "MII", "PRIVATE KEY"] {
            assert!(!rendered.contains(sentinel), "{name}: {rendered}");
        }
        assert!(!text.is_empty(), "{name} is empty");
    }
}
