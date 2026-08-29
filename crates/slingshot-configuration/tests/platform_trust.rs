//! Assertions for the platform trust snapshot.
//!
//! A platform trust store is a list of decisions, not a list of certificates.
//! Every decision a store can express is scripted here: denied, restricted to
//! something, uninterpretable, and two records for the same bytes that
//! disagree. None of those can be arranged on the machine running the test, and
//! reducing any of them to the bytes alone would silently widen provider
//! policy.
//!
//! The current row also takes its real snapshot, which is an observation about
//! this environment and nothing else.

use std::path::PathBuf;

use slingshot_configuration::platform_trust::{
    PlatformTrustSnapshot, PlatformTrustSource, ProviderDecision, ProviderRecord,
};
use slingshot_configuration::profile_loader::{
    ConfigurationDiagnostic, DiagnosticSourceClass, DiagnosticStage,
};
use slingshot_domain::profile_authentication_contract::ConfigurationFailureCode;

/// Directory holding the committed certificates.
const CERTIFICATE_FIXTURES: &str =
    "../slingshot-test-support/fixtures/additional-certificate-authority";

/// Label every current-environment observation carries.
const UNTRUSTED_LABEL: &str = "untrusted_current_native_observation";

/// A trust store that holds exactly what a test scripts.
struct ScriptedStore {
    /// Records the store holds, or the failure enumerating it produces.
    answer: Result<Vec<ProviderRecord>, ConfigurationDiagnostic>,
}

impl PlatformTrustSource for ScriptedStore {
    fn records(&self) -> Result<Vec<ProviderRecord>, ConfigurationDiagnostic> {
        self.answer.clone()
    }
}

/// Returns the certificates one committed source holds.
///
/// The decoding here is deliberately unconditional, because some of these
/// sources are exactly the ones an author trust extension refuses: a store may
/// still claim them, and what happens then is what this file is about.
fn certificates(name: &str) -> Vec<Vec<u8>> {
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD;

    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(CERTIFICATE_FIXTURES).join(name);
    let text = std::fs::read_to_string(&path).expect("the certificate source reads");
    let mut decoded = Vec::new();
    let mut encoded = String::new();
    let mut inside = false;
    for line in text.lines() {
        if line.starts_with("-----BEGIN CERTIFICATE-----") {
            inside = true;
            continue;
        }
        if line.starts_with("-----END CERTIFICATE-----") {
            decoded.push(STANDARD.decode(encoded.as_bytes()).expect("the block decodes"));
            encoded.clear();
            inside = false;
            continue;
        }
        if inside {
            encoded.push_str(line.trim());
        }
    }
    assert!(!decoded.is_empty(), "{name} holds no certificate");
    decoded
}

/// Returns a store holding `records`.
fn store(records: Vec<ProviderRecord>) -> ScriptedStore {
    ScriptedStore { answer: Ok(records) }
}

/// Returns one record carrying `decision`.
fn record(der: &[u8], decision: ProviderDecision) -> ProviderRecord {
    ProviderRecord { der: der.to_vec(), decision }
}

#[test]
fn only_an_unconditional_decision_is_retained() {
    let roots = certificates("two-authorities.pem");
    let trusted = store(
        roots
            .iter()
            .map(|der| record(der, ProviderDecision::UnconditionallyTrustedForServerAuthentication))
            .collect(),
    );
    let snapshot = PlatformTrustSnapshot::take(&trusted).expect("unconditional roots are retained");
    let mut expected = roots.clone();
    expected.sort();
    assert_eq!(snapshot.roots(), expected, "the snapshot is not in one order");

    for refused in [
        ProviderDecision::Distrusted,
        ProviderDecision::ExternallyRestricted,
        ProviderDecision::Unevaluable,
    ] {
        let mixed = store(vec![
            record(&roots[0], ProviderDecision::UnconditionallyTrustedForServerAuthentication),
            record(&roots[1], refused),
        ]);
        let diagnostic = PlatformTrustSnapshot::take(&mixed)
            .map_or_else(|diagnostic| diagnostic, |_| panic!("{refused:?} was retained"));
        assert_eq!(diagnostic.code, ConfigurationFailureCode::PlatformTrustSnapshotInvalid);
        assert_eq!(diagnostic.source_class, DiagnosticSourceClass::PlatformTrust);
        assert_eq!(diagnostic.stage, DiagnosticStage::SnapshotConstruction);
    }
}

#[test]
fn two_records_for_one_certificate_that_disagree_fail_the_whole_snapshot() {
    let roots = certificates("one-authority.pem");
    let agreeing = store(vec![
        record(&roots[0], ProviderDecision::UnconditionallyTrustedForServerAuthentication),
        record(&roots[0], ProviderDecision::UnconditionallyTrustedForServerAuthentication),
    ]);
    let snapshot = PlatformTrustSnapshot::take(&agreeing).expect("agreeing records are retained");
    assert_eq!(snapshot.roots().len(), 1, "an agreeing duplicate was retained twice");

    let conflicting = store(vec![
        record(&roots[0], ProviderDecision::UnconditionallyTrustedForServerAuthentication),
        record(&roots[0], ProviderDecision::Distrusted),
    ]);
    assert!(
        PlatformTrustSnapshot::take(&conflicting).is_err(),
        "a conflicting duplicate was resolved rather than refused"
    );
}

#[test]
fn a_retained_root_must_be_an_authority_that_may_authenticate_a_server() {
    for name in ["end-entity.pem", "other-purpose.pem"] {
        let ineligible = certificates(name);
        let claimed = store(vec![record(
            &ineligible[0],
            ProviderDecision::UnconditionallyTrustedForServerAuthentication,
        )]);
        assert!(
            PlatformTrustSnapshot::take(&claimed).is_err(),
            "{name} was retained on the store's word alone"
        );
    }
}

#[test]
fn a_store_that_cannot_be_enumerated_produces_one_diagnostic() {
    let refusal = ConfigurationDiagnostic::once(
        DiagnosticSourceClass::PlatformTrust,
        DiagnosticStage::SnapshotConstruction,
        "platform_trust",
        ConfigurationFailureCode::PlatformTrustSnapshotInvalid,
    );
    let broken = ScriptedStore { answer: Err(refusal.clone()) };
    assert_eq!(PlatformTrustSnapshot::take(&broken).expect_err("it refuses"), refusal);
}

#[test]
fn an_empty_store_is_a_snapshot_of_nothing_rather_than_a_failure() {
    let empty = store(Vec::new());
    let snapshot = PlatformTrustSnapshot::take(&empty).expect("an empty store is a snapshot");
    assert!(snapshot.roots().is_empty());
}

#[test]
fn this_row_takes_its_own_snapshot_once() {
    use slingshot_configuration::platform_trust::OperatingSystemTrustSource;

    let Ok(snapshot) = PlatformTrustSnapshot::take(&OperatingSystemTrustSource) else {
        return;
    };
    let again = PlatformTrustSnapshot::take(&OperatingSystemTrustSource)
        .expect("the store answers twice the same way");
    assert_eq!(snapshot, again, "two snapshots of one store disagree");
    println!("{UNTRUSTED_LABEL}: {} roots", snapshot.roots().len());
}
