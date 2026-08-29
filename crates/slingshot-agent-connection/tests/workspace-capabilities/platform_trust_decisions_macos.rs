//! Probe for the macOS provider-trust-decision capability.
//!
//! Requires enumerating the platform provider records with the decision each
//! one carries, so a store can tell an unconditionally trusted root from one
//! that is denied, restricted to another purpose, or left unevaluable, instead
//! of flattening every record into one undifferentiated list.

use security_framework::trust_settings::{Domain, TrustSettings, TrustSettingsForCertificate};

/// Domains a credential check reads a decision from, in precedence order.
const DECISION_DOMAINS: [Domain; 3] = [Domain::User, Domain::Admin, Domain::System];

#[test]
fn every_platform_record_carries_a_decision_that_is_not_flattened() {
    let mut unconditional = 0_usize;
    let mut denied = 0_usize;
    let mut unevaluable = 0_usize;
    let mut enumerated = 0_usize;

    for domain in DECISION_DOMAINS {
        let settings = TrustSettings::new(domain);
        let Ok(records) = settings.iter() else { continue };
        for record in records {
            enumerated += 1;
            let encoded = record.to_der();
            assert!(!encoded.is_empty(), "every record exposes its encoded bytes");
            match settings.tls_trust_settings_for_certificate(&record) {
                Ok(Some(TrustSettingsForCertificate::TrustRoot)) => unconditional += 1,
                Ok(Some(TrustSettingsForCertificate::TrustAsRoot)) => unconditional += 1,
                Ok(Some(TrustSettingsForCertificate::Deny)) => denied += 1,
                Ok(Some(TrustSettingsForCertificate::Unspecified)) => unevaluable += 1,
                Ok(Some(TrustSettingsForCertificate::Invalid)) => unevaluable += 1,
                Ok(None) => unevaluable += 1,
                Err(_) => unevaluable += 1,
            }
        }
    }

    assert!(enumerated > 0, "the platform holds provider records");
    assert_eq!(
        unconditional + denied + unevaluable,
        enumerated,
        "every enumerated record carries exactly one decision"
    );
}
