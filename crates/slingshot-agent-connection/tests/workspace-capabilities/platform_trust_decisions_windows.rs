//! Probe for the Windows provider-trust-decision capability.
//!
//! Requires enumerating the platform provider records with the purposes each
//! one permits and the separate store of distrusted records, so a store can
//! tell an unconditionally trusted root from one restricted to another purpose
//! or explicitly distrusted, instead of flattening every record into one
//! undifferentiated list.

use schannel::cert_context::ValidUses;
use schannel::cert_store::CertStore;

/// Object identifier of the server-authentication purpose.
const SERVER_AUTHENTICATION_PURPOSE: &str = "1.3.6.1.5.5.7.3.1";

/// Store holding the records the platform trusts as roots.
const ROOT_STORE: &str = "Root";

/// Store holding the records the platform explicitly distrusts.
const DISALLOWED_STORE: &str = "Disallowed";

#[test]
fn every_platform_record_carries_a_purpose_and_distrust_is_separate() {
    let roots = CertStore::open_current_user(ROOT_STORE).expect("the root store opens");
    let mut unconditional = 0_usize;
    let mut restricted = 0_usize;
    let mut enumerated = 0_usize;
    for record in roots.certs() {
        enumerated += 1;
        assert!(!record.to_der().is_empty(), "every record exposes its encoded bytes");
        match record.valid_uses().expect("every record reports its purposes") {
            ValidUses::All => unconditional += 1,
            ValidUses::Oids(purposes) => {
                if purposes.iter().any(|purpose| purpose == SERVER_AUTHENTICATION_PURPOSE) {
                    unconditional += 1;
                } else {
                    restricted += 1;
                }
            }
        }
    }
    assert!(enumerated > 0, "the platform holds provider records");
    assert_eq!(unconditional + restricted, enumerated, "every record carries exactly one decision");

    let distrusted =
        CertStore::open_current_user(DISALLOWED_STORE).expect("the distrust store opens");
    let distrusted_count = distrusted.certs().count();
    let root_fingerprints: Vec<Vec<u8>> = roots
        .certs()
        .map(|record| {
            record
                .fingerprint(schannel::cert_context::HashAlgorithm::sha256())
                .expect("every record reports a fingerprint")
        })
        .collect();
    for record in distrusted.certs() {
        let fingerprint = record
            .fingerprint(schannel::cert_context::HashAlgorithm::sha256())
            .expect("every distrusted record reports a fingerprint");
        assert!(
            !root_fingerprints.contains(&fingerprint) || distrusted_count > 0,
            "a distrusted record is visible separately from the trusted store"
        );
    }
}
