//! Assertions for the platform trust snapshot.
//!
//! A platform trust store is a list of decisions, not a list of certificates.
//! Every decision a store can express is scripted here: denied, restricted to
//! something, uninterpretable, and two records for the same bytes that
//! disagree. None of those can be arranged on the machine running the test, and
//! reducing any of them to the bytes alone would silently widen provider
//! policy. Each is left out of the snapshot while the rest of the store is
//! still retained, because an ordinary managed machine holds such records.
//!
//! The current row also takes its real snapshot. How many roots it retains is
//! an observation about this environment; that it can take one at all is not.

use std::path::PathBuf;

use slingshot_configuration::platform_trust::{
    PlatformTrustSnapshot, PlatformTrustSource, ProviderDecision, ProviderRecord,
};
use slingshot_configuration::profile_loader::{
    ConfigurationDiagnostic, DiagnosticSourceClass, DiagnosticStage,
};
use slingshot_domain::profile_authentication_contract::{
    ConfigurationFailureCode, ProfileAuthenticationContract,
};

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

    for left_out in [
        ProviderDecision::Distrusted,
        ProviderDecision::ExternallyRestricted,
        ProviderDecision::Unevaluable,
    ] {
        let mixed = store(vec![
            record(&roots[0], ProviderDecision::UnconditionallyTrustedForServerAuthentication),
            record(&roots[1], left_out),
        ]);
        let snapshot = PlatformTrustSnapshot::take(&mixed).unwrap_or_else(|diagnostic| {
            panic!("{left_out:?} failed the snapshot: {diagnostic:?}")
        });
        assert_eq!(snapshot.roots(), [roots[0].clone()], "{left_out:?} was retained");
    }
}

#[test]
fn two_records_for_one_certificate_that_disagree_leave_it_out() {
    let roots = certificates("one-authority.pem");
    let agreeing = store(vec![
        record(&roots[0], ProviderDecision::UnconditionallyTrustedForServerAuthentication),
        record(&roots[0], ProviderDecision::UnconditionallyTrustedForServerAuthentication),
    ]);
    let snapshot = PlatformTrustSnapshot::take(&agreeing).expect("agreeing records are retained");
    assert_eq!(snapshot.roots().len(), 1, "an agreeing duplicate was retained twice");

    let other = certificates("two-authorities.pem")
        .into_iter()
        .find(|der| *der != roots[0])
        .expect("a second authority is committed");
    for (first, second) in [
        (
            ProviderDecision::UnconditionallyTrustedForServerAuthentication,
            ProviderDecision::Distrusted,
        ),
        (
            ProviderDecision::Distrusted,
            ProviderDecision::UnconditionallyTrustedForServerAuthentication,
        ),
    ] {
        let conflicting = store(vec![
            record(&roots[0], first),
            record(&other, ProviderDecision::UnconditionallyTrustedForServerAuthentication),
            record(&roots[0], second),
            record(&roots[0], ProviderDecision::UnconditionallyTrustedForServerAuthentication),
        ]);
        let snapshot =
            PlatformTrustSnapshot::take(&conflicting).expect("a conflict leaves the rest usable");
        assert_eq!(
            snapshot.roots(),
            std::slice::from_ref(&other),
            "a conflicting duplicate was resolved rather than left out"
        );
    }
}

#[test]
fn a_retained_root_must_be_an_authority_that_may_authenticate_a_server() {
    let eligible = certificates("one-authority.pem");
    let mut claims: Vec<(String, Vec<u8>)> = ["end-entity.pem", "other-purpose.pem"]
        .into_iter()
        .map(|name| (name.to_owned(), certificates(name)[0].clone()))
        .collect();
    claims.push(("bytes that are not a certificate".to_owned(), b"not a certificate".to_vec()));
    let mut trailing = eligible[0].clone();
    trailing.push(0);
    claims.push(("a certificate followed by trailing bytes".to_owned(), trailing));
    for (name, claimed) in claims {
        let mixed = store(vec![
            record(&claimed, ProviderDecision::UnconditionallyTrustedForServerAuthentication),
            record(&eligible[0], ProviderDecision::UnconditionallyTrustedForServerAuthentication),
        ]);
        let snapshot = PlatformTrustSnapshot::take(&mixed)
            .unwrap_or_else(|diagnostic| panic!("{name} failed the snapshot: {diagnostic:?}"));
        assert_eq!(
            snapshot.roots(),
            [eligible[0].clone()],
            "{name} was retained on the store's word alone"
        );
    }
}

#[test]
fn a_record_beyond_the_per_authority_bound_is_left_out() {
    let limit = usize::try_from(
        ProfileAuthenticationContract::embedded().limits.maximum_platform_trust_authority_der_bytes,
    )
    .expect("the bound is addressable");
    let eligible = certificates("one-authority.pem");
    let oversized = vec![0_u8; limit + 1];
    let mixed = store(vec![
        record(&oversized, ProviderDecision::UnconditionallyTrustedForServerAuthentication),
        record(&eligible[0], ProviderDecision::UnconditionallyTrustedForServerAuthentication),
    ]);
    let snapshot = PlatformTrustSnapshot::take(&mixed).expect("an oversized record is left out");
    assert_eq!(snapshot.roots(), [eligible[0].clone()]);
}

#[test]
fn a_store_of_many_unusable_records_is_still_a_snapshot() {
    let maximum = usize::try_from(
        ProfileAuthenticationContract::embedded().limits.maximum_platform_trust_authorities,
    )
    .expect("the bound is addressable");
    let eligible = certificates("one-authority.pem");
    let mut records: Vec<ProviderRecord> = (0..=maximum)
        .map(|index| {
            record(
                &index.to_be_bytes(),
                ProviderDecision::UnconditionallyTrustedForServerAuthentication,
            )
        })
        .collect();
    records.push(record(
        &eligible[0],
        ProviderDecision::UnconditionallyTrustedForServerAuthentication,
    ));
    let snapshot = PlatformTrustSnapshot::take(&store(records))
        .expect("records that are left out do not count toward the authority bound");
    assert_eq!(snapshot.roots(), [eligible[0].clone()]);
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

    // A daemon cannot start on a row whose store does not snapshot, so a
    // failure here is a failure rather than an observation to skip.
    let snapshot = PlatformTrustSnapshot::take(&OperatingSystemTrustSource)
        .expect("this row's platform trust store snapshots");
    let again = PlatformTrustSnapshot::take(&OperatingSystemTrustSource)
        .expect("the store answers twice the same way");
    assert_eq!(snapshot, again, "two snapshots of one store disagree");
    println!("{UNTRUSTED_LABEL}: {} roots", snapshot.roots().len());
}
