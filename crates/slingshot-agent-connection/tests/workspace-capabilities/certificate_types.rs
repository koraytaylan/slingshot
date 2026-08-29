//! Probe for the certificate-types capability.
//!
//! Requires reading Privacy Enhanced Mail input into owned encoded bytes, an
//! iterator over a bundle, refusal of malformed input, and a server name that
//! rejects an address that is not a name.

use rustls_pki_types::pem::PemObject;
use rustls_pki_types::{CertificateDer, ServerName, UnixTime};

use crate::material;

#[test]
fn provider_records_read_into_owned_encoded_bytes() {
    let author_root = material::certificate("author-root");
    assert!(!author_root.as_ref().is_empty(), "the record carries encoded bytes");
    assert_eq!(author_root.as_ref()[0], 0x30, "the record opens a sequence");

    let bundle =
        std::fs::read(material::certificate_path("author-root.pem")).expect("the bundle reads");
    let mut combined = bundle.clone();
    combined.extend_from_slice(
        &std::fs::read(material::certificate_path("identity-management-root.pem"))
            .expect("the bundle reads"),
    );
    let records: Vec<CertificateDer<'static>> = CertificateDer::pem_slice_iter(&combined)
        .collect::<Result<Vec<_>, _>>()
        .expect("a bundle of two records reads");
    assert_eq!(records.len(), 2);
    assert_ne!(records[0], records[1]);

    assert!(CertificateDer::from_pem_slice(b"not a record").is_err(), "malformed input is refused");

    let name = ServerName::try_from("author.example.invalid").expect("a host name is accepted");
    assert!(matches!(name, ServerName::DnsName(_)));
    let address = ServerName::try_from("127.0.0.1").expect("an address parses");
    assert!(matches!(address, ServerName::IpAddress(_)), "an address is not a name");
    assert!(ServerName::try_from("not a host").is_err(), "an invalid name is refused");

    let now = UnixTime::now();
    assert!(now.as_secs() > 0, "the clock reports a time the verifier can use");
}
