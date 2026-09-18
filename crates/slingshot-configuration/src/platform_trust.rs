//! Snapshot of the platform server-authentication trust store.
//!
//! A platform trust store is not a list of certificates; it is a list of
//! decisions. The same certificate can be present and denied, present and
//! restricted to one application or one name, or present with settings this
//! build cannot interpret. Reducing all of that to "here are the bytes" would
//! quietly widen provider policy, because a verifier built from bytes alone has
//! no way to reproduce a restriction that lived outside them.
//!
//! So a record is retained only when the store says, unconditionally, that this
//! authority may authenticate a server - and when every record for the same
//! bytes says the same thing. Every other record is left out: a denied,
//! restricted, or uninterpretable record, bytes that are not a certificate
//! authority for server authentication, and bytes two records disagree about.
//! Leaving a record out can only narrow what a verifier accepts, never widen
//! it, whereas refusing the whole snapshot would leave a host whose store holds
//! one such record - an ordinary managed machine - unable to start at all.
//!
//! The snapshot is taken once. Nothing here reopens the store, so editing the
//! platform's trust after startup cannot affect a running client; a restart
//! takes a new snapshot, and the revision that snapshot produces differs.

use slingshot_domain::profile_authentication_contract::{
    ConfigurationFailureCode, ProfileAuthenticationContract,
};
use x509_parser::prelude::{FromDer, X509Certificate};

use crate::profile_loader::{ConfigurationDiagnostic, DiagnosticSourceClass, DiagnosticStage};

/// Structural location every decision here is reported at.
const LOCATION: &str = "platform_trust";
#[cfg(target_os = "linux")]
const TRUST_BUNDLE_BYTE_MULTIPLIER: u64 = 2;
#[cfg(target_os = "linux")]
const TRUST_BUNDLE_ENTRY_MULTIPLIER: u64 = 4;

/// What one provider store says about one record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderDecision {
    /// The record may authenticate a server, with no condition attached.
    UnconditionallyTrustedForServerAuthentication,
    /// The record is denied or distrusted.
    Distrusted,
    /// The record is trusted only for some application, policy, or name.
    ExternallyRestricted,
    /// The record carries settings this build cannot interpret.
    Unevaluable,
}

/// One record a provider store holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderRecord {
    /// Exact bytes of the record.
    pub der: Vec<u8>,
    /// What the store says about it.
    pub decision: ProviderDecision,
}

/// Enumerates the records one platform trust store holds.
///
/// The trait exists so every decision a store can express is provable without
/// that store: a test supplies a denied record, a restricted one, and two
/// conflicting records for the same bytes, none of which this machine can be
/// asked to produce.
pub trait PlatformTrustSource {
    /// Returns every record the store holds.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigurationFailureCode::PlatformTrustSnapshotInvalid`] when
    /// the store cannot be enumerated at all.
    fn records(&self) -> Result<Vec<ProviderRecord>, ConfigurationDiagnostic>;
}

/// Why one distinct certificate was left out of a snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LeftOutReason {
    /// Every record for the bytes denies or distrusts them.
    Distrusted,
    /// The store restricts the bytes to some application, policy, or name.
    ExternallyRestricted,
    /// The store holds settings for the bytes this build cannot interpret.
    Unevaluable,
    /// Two records for the same bytes carry different decisions.
    ConflictingDecisions,
    /// The bytes exceed the contract's per-authority bound.
    Oversized,
    /// The bytes are not exactly one certificate.
    Unparseable,
    /// The certificate is not an authority that may authenticate a server.
    NotServerAuthenticationAuthority,
}

impl LeftOutReason {
    /// Returns how a startup log names this reason.
    #[must_use]
    pub fn as_text(self) -> &'static str {
        match self {
            Self::Distrusted => "distrusted",
            Self::ExternallyRestricted => "restricted by the store",
            Self::Unevaluable => "with settings this build cannot evaluate",
            Self::ConflictingDecisions => "with conflicting decisions",
            Self::Oversized => "over the per-authority size bound",
            Self::Unparseable => "not a parseable certificate",
            Self::NotServerAuthenticationAuthority => {
                "not a certificate authority for server authentication"
            }
        }
    }
}

/// How many distinct certificates a snapshot left out, by reason.
///
/// Counts only: which certificate was left out stays on the machine that holds
/// it, so a startup log can say that something was dropped and why without
/// naming or fingerprinting anything in the store.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LeftOutRecords {
    /// Distinct certificates per reason; a reason with none is absent.
    counts: std::collections::BTreeMap<LeftOutReason, u64>,
}

impl LeftOutRecords {
    /// Returns how many distinct certificates were left out in total.
    #[must_use]
    pub fn total(&self) -> u64 {
        self.counts.values().sum()
    }

    /// Returns how many distinct certificates were left out for `reason`.
    #[must_use]
    pub fn count(&self, reason: LeftOutReason) -> u64 {
        self.counts.get(&reason).copied().unwrap_or_default()
    }

    /// Counts one more certificate left out for `reason`.
    fn record(&mut self, reason: LeftOutReason) {
        let count = self.counts.entry(reason).or_default();
        *count = count.saturating_add(1);
    }
}

impl ::core::fmt::Display for LeftOutRecords {
    /// Writes each reason with its count, in a fixed order, separated by commas.
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let mut separator = "";
        for (reason, count) in &self.counts {
            write!(formatter, "{separator}{count} {}", reason.as_text())?;
            separator = ", ";
        }
        Ok(())
    }
}

/// The immutable platform roots one startup accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformTrustSnapshot {
    /// Distinct retained roots, in ascending byte order.
    roots: Vec<Vec<u8>>,
    /// What was left out, counted by reason.
    left_out: LeftOutRecords,
}

impl PlatformTrustSnapshot {
    /// Takes one snapshot from `source`.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigurationFailureCode::PlatformTrustSnapshotInvalid`] when
    /// the store cannot be enumerated, or when the roots it retains exceed the
    /// contract's count or aggregate bounds. A record that is not retained is
    /// not a failure; see [`PlatformTrustSnapshot::roots`]. No record byte,
    /// subject, or provider message survives.
    pub fn take(source: &dyn PlatformTrustSource) -> Result<Self, ConfigurationDiagnostic> {
        let limits = &ProfileAuthenticationContract::embedded().limits;
        let records = source.records()?;
        // Each distinct certificate with its decision, or with none when two
        // records disagree: the platform's two answers cannot be reconciled
        // without choosing one.
        let mut decided: Vec<(Vec<u8>, Option<ProviderDecision>)> = Vec::new();
        for record in records {
            match decided.iter_mut().find(|(der, _)| *der == record.der) {
                Some((_, decision)) if *decision != Some(record.decision) => *decision = None,
                Some(_) => {}
                None => decided.push((record.der, Some(record.decision))),
            }
        }
        let mut roots = Vec::new();
        let mut left_out = LeftOutRecords::default();
        for (der, decision) in decided {
            match left_out_reason(&der, decision, limits.maximum_platform_trust_authority_der_bytes)
            {
                Some(reason) => left_out.record(reason),
                None => roots.push(der),
            }
        }
        roots.sort();
        if u64::try_from(roots.len()).unwrap_or(u64::MAX)
            > limits.maximum_platform_trust_authorities
        {
            return Err(refusal());
        }
        let aggregate: u64 = roots
            .iter()
            .try_fold(0_u64, |total, root| total.checked_add(u64::try_from(root.len()).ok()?))
            .ok_or_else(refusal)?;
        if aggregate > limits.maximum_identity_management_trust_canonical_bytes {
            return Err(refusal());
        }
        Ok(Self { roots, left_out })
    }

    /// Returns how many distinct certificates were left out, by reason.
    #[must_use]
    pub fn left_out(&self) -> &LeftOutRecords {
        &self.left_out
    }

    /// Returns the retained roots, in ascending byte order.
    ///
    /// A root is retained only when every record for its bytes is
    /// unconditionally trusted for server authentication, its bytes are within
    /// the contract's per-authority bound, and they parse as a certificate
    /// authority whose Extended Key Usage, when present, includes server
    /// authentication. Every other record is absent from this list.
    #[must_use]
    pub fn roots(&self) -> &[Vec<u8>] {
        &self.roots
    }
}

/// Returns why one distinct certificate is left out, or nothing when it is
/// retained.
fn left_out_reason(
    der: &[u8],
    decision: Option<ProviderDecision>,
    maximum_der_bytes: u64,
) -> Option<LeftOutReason> {
    let reason = match decision {
        None => LeftOutReason::ConflictingDecisions,
        Some(ProviderDecision::Distrusted) => LeftOutReason::Distrusted,
        Some(ProviderDecision::ExternallyRestricted) => LeftOutReason::ExternallyRestricted,
        Some(ProviderDecision::Unevaluable) => LeftOutReason::Unevaluable,
        Some(ProviderDecision::UnconditionallyTrustedForServerAuthentication) => {
            if u64::try_from(der.len()).unwrap_or(u64::MAX) > maximum_der_bytes {
                LeftOutReason::Oversized
            } else {
                match anchor_is_eligible(der) {
                    Ok(true) => return None,
                    Ok(false) => LeftOutReason::NotServerAuthenticationAuthority,
                    Err(_) => LeftOutReason::Unparseable,
                }
            }
        }
    };
    Some(reason)
}

/// Requires one bundle anchor to be an authority that may authenticate a
/// server, so accepted bytes fully represent the decision they came with.
#[cfg(target_os = "linux")]
fn require_eligible_anchor(der: &[u8]) -> Result<(), ConfigurationDiagnostic> {
    if anchor_is_eligible(der)? {
        return Ok(());
    }
    Err(refusal())
}

/// Returns whether one valid certificate is an eligible trust anchor.
///
/// A platform trust directory can contain valid end-entity certificates (for
/// example, a host's generated snake-oil certificate) beside its CA anchors.
/// Those are not trust records and are ignored when reading a directory. A
/// malformed certificate is an error, so a bundle file holding corrupted data
/// is refused rather than partly read; a snapshot never retains either kind.
fn anchor_is_eligible(der: &[u8]) -> Result<bool, ConfigurationDiagnostic> {
    let (remainder, certificate) = X509Certificate::from_der(der).map_err(|_| refusal())?;
    if !remainder.is_empty() {
        return Err(refusal());
    }
    let authority = certificate
        .basic_constraints()
        .map_err(|_| refusal())?
        .is_some_and(|extension| extension.value.ca);
    if !authority {
        return Ok(false);
    }
    let authenticates_servers = certificate
        .extended_key_usage()
        .map_err(|_| refusal())?
        .is_none_or(|extension| extension.value.server_auth || extension.value.any);
    Ok(authenticates_servers)
}

/// Returns the one diagnostic a platform snapshot failure reports.
fn refusal() -> ConfigurationDiagnostic {
    ConfigurationDiagnostic::once(
        DiagnosticSourceClass::PlatformTrust,
        DiagnosticStage::SnapshotConstruction,
        LOCATION,
        ConfigurationFailureCode::PlatformTrustSnapshotInvalid,
    )
}

/// The trust store of the row this build runs on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OperatingSystemTrustSource;

#[cfg(target_os = "linux")]
impl PlatformTrustSource for OperatingSystemTrustSource {
    fn records(&self) -> Result<Vec<ProviderRecord>, ConfigurationDiagnostic> {
        // Only provider locations, never SSL_CERT_FILE/SSL_CERT_DIR supplied by
        // the process that launched this daemon.
        let mut candidates: Vec<std::path::PathBuf> =
            openssl_probe::candidate_cert_dirs().map(std::path::Path::to_path_buf).collect();
        candidates.extend(
            [
                "/etc/pki/ca-trust/extracted/pem/tls-ca-bundle.pem",
                "/etc/ssl/ca-bundle.pem",
                "/etc/pki/tls/cacert.pem",
                "/etc/ssl/cert.pem",
            ]
            .into_iter()
            .map(std::path::PathBuf::from)
            .filter(|path| path.exists()),
        );
        let mut roots = std::collections::BTreeSet::new();
        let limits = &ProfileAuthenticationContract::embedded().limits;
        let mut aggregate = 0_u64;
        for path in candidates {
            let directory = path.is_dir();
            read_bundle(&path, |source| {
                let parsed = if directory {
                    parse_platform_directory_bundle(source)?
                } else {
                    parse_platform_bundle(source)?
                };
                for der in parsed {
                    if roots.insert(der.clone()) {
                        aggregate = aggregate.checked_add(der.len() as u64).ok_or_else(refusal)?;
                        if roots.len() as u64 > limits.maximum_platform_trust_authorities
                            || aggregate > limits.maximum_identity_management_trust_canonical_bytes
                        {
                            return Err(refusal());
                        }
                    }
                }
                Ok(())
            })?;
        }
        Ok(roots
            .into_iter()
            .map(|der| ProviderRecord {
                der,
                decision: ProviderDecision::UnconditionallyTrustedForServerAuthentication,
            })
            .collect())
    }
}

#[cfg(target_os = "linux")]
fn parse_platform_bundle(source: &[u8]) -> Result<Vec<Vec<u8>>, ConfigurationDiagnostic> {
    let roots = parse_platform_bundle_raw(source)?;
    for der in &roots {
        require_eligible_anchor(der)?;
    }
    Ok(roots)
}

#[cfg(target_os = "linux")]
fn parse_platform_directory_bundle(source: &[u8]) -> Result<Vec<Vec<u8>>, ConfigurationDiagnostic> {
    parse_platform_bundle_raw(source)?
        .into_iter()
        .filter_map(|der| match anchor_is_eligible(&der) {
            Ok(true) => Some(Ok(der)),
            Ok(false) => None,
            Err(failure) => Some(Err(failure)),
        })
        .collect::<Result<Vec<_>, _>>()
}

#[cfg(target_os = "linux")]
fn parse_platform_bundle_raw(source: &[u8]) -> Result<Vec<Vec<u8>>, ConfigurationDiagnostic> {
    let limits = &ProfileAuthenticationContract::embedded().limits;
    if source.len() as u64
        > limits
            .maximum_identity_management_trust_canonical_bytes
            .saturating_mul(TRUST_BUNDLE_BYTE_MULTIPLIER)
    {
        return Err(refusal());
    }
    let text = core::str::from_utf8(source).map_err(|_| refusal())?;
    let roots = crate::additional_certificate_authority::read_blocks(
        text,
        limits.maximum_platform_trust_authorities,
        limits.maximum_platform_trust_authority_der_bytes,
    )
    .map_err(|_| refusal())?;
    if roots.is_empty() || roots.len() as u64 > limits.maximum_platform_trust_authorities {
        return Err(refusal());
    }
    for der in &roots {
        if der.len() as u64 > limits.maximum_platform_trust_authority_der_bytes {
            return Err(refusal());
        }
    }
    Ok(roots)
}

#[cfg(all(test, target_os = "linux"))]
mod linux_bundle_tests {
    use super::*;

    const AUTHORITY: &str = include_str!(
        "../../slingshot-test-support/fixtures/additional-certificate-authority/one-authority.pem"
    );

    #[test]
    fn platform_bundles_use_platform_bounds_and_retain_strict_validation() {
        let count = ProfileAuthenticationContract::embedded()
            .limits
            .maximum_additional_certificate_authorities as usize
            + 1;
        let bundle = AUTHORITY.repeat(count);
        assert_eq!(parse_platform_bundle(bundle.as_bytes()).unwrap().len(), count);
        assert!(
            crate::additional_certificate_authority::AdditionalAuthorCertificates::parse(
                bundle.as_bytes()
            )
            .is_err()
        );
        for source in [
            "",
            "not a certificate",
            include_str!(
                "../../slingshot-test-support/fixtures/additional-certificate-authority/with-private-key.pem"
            ),
            include_str!(
                "../../slingshot-test-support/fixtures/additional-certificate-authority/end-entity.pem"
            ),
        ] {
            assert!(parse_platform_bundle(source.as_bytes()).is_err());
        }
        assert!(
            parse_platform_directory_bundle(
                include_str!(
                    "../../slingshot-test-support/fixtures/additional-certificate-authority/end-entity.pem"
                )
                .as_bytes(),
            )
            .expect("a valid end-entity certificate is parseable")
            .is_empty(),
            "a directory source retained a non-CA certificate"
        );
        let too_many = AUTHORITY.repeat(
            ProfileAuthenticationContract::embedded().limits.maximum_platform_trust_authorities
                as usize
                + 1,
        );
        assert!(parse_platform_bundle(too_many.as_bytes()).is_err());
    }
}

/// Returns every bundle file at or below `path`.
///
/// This row expresses its decisions by which certificates are in the bundle at
/// all, so a record's presence is its unconditional decision.
#[cfg(target_os = "linux")]
fn read_bundle(
    path: &std::path::Path,
    mut consume: impl FnMut(&[u8]) -> Result<(), ConfigurationDiagnostic>,
) -> Result<(), ConfigurationDiagnostic> {
    use std::io::Read as _;
    let limits = &ProfileAuthenticationContract::embedded().limits;
    let maximum = limits
        .maximum_identity_management_trust_canonical_bytes
        .saturating_mul(TRUST_BUNDLE_BYTE_MULTIPLIER);
    let read = |path: &std::path::Path| {
        let file = std::fs::File::open(path).map_err(|_| refusal())?;
        let mut bytes = Vec::new();
        file.take(maximum + 1).read_to_end(&mut bytes).map_err(|_| refusal())?;
        if bytes.len() as u64 > maximum {
            return Err(refusal());
        }
        Ok(bytes)
    };
    if path.is_file() {
        return consume(&read(path)?);
    }
    let entries = std::fs::read_dir(path).map_err(|_| refusal())?;
    let mut bytes = 0_u64;
    for (index, entry) in entries.enumerate() {
        if index as u64
            >= limits
                .maximum_platform_trust_authorities
                .saturating_mul(TRUST_BUNDLE_ENTRY_MULTIPLIER)
        {
            return Err(refusal());
        }
        let entry = entry.map_err(|_| refusal())?;
        let metadata = entry.metadata().map_err(|_| refusal())?;
        let metadata = if metadata.is_symlink() {
            std::fs::metadata(entry.path()).map_err(|_| refusal())?
        } else {
            metadata
        };
        if metadata.is_dir() {
            continue;
        }
        if !metadata.is_file() {
            return Err(refusal());
        }
        let source = read(&entry.path())?;
        bytes = bytes.checked_add(source.len() as u64).ok_or_else(refusal)?;
        if bytes > maximum.saturating_mul(TRUST_BUNDLE_ENTRY_MULTIPLIER) {
            return Err(refusal());
        }
        consume(&source)?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
impl PlatformTrustSource for OperatingSystemTrustSource {
    fn records(&self) -> Result<Vec<ProviderRecord>, ConfigurationDiagnostic> {
        use security_framework::trust_settings::{
            Domain, TrustSettings, TrustSettingsForCertificate,
        };

        let mut records = Vec::new();
        for domain in [Domain::System, Domain::Admin, Domain::User] {
            let settings = TrustSettings::new(domain);
            let Ok(certificates) = settings.iter() else {
                return Err(refusal());
            };
            for certificate in certificates {
                let decision = match settings.tls_trust_settings_for_certificate(&certificate) {
                    Ok(None) => ProviderDecision::UnconditionallyTrustedForServerAuthentication,
                    Ok(Some(
                        TrustSettingsForCertificate::TrustRoot
                        | TrustSettingsForCertificate::TrustAsRoot,
                    )) => ProviderDecision::UnconditionallyTrustedForServerAuthentication,
                    Ok(Some(TrustSettingsForCertificate::Deny)) => ProviderDecision::Distrusted,
                    Ok(Some(_)) => ProviderDecision::ExternallyRestricted,
                    Err(_) => ProviderDecision::Unevaluable,
                };
                records.push(ProviderRecord { der: certificate.to_der(), decision });
            }
        }
        Ok(records)
    }
}

#[cfg(target_os = "windows")]
impl PlatformTrustSource for OperatingSystemTrustSource {
    fn records(&self) -> Result<Vec<ProviderRecord>, ConfigurationDiagnostic> {
        use schannel::cert_store::CertStore;

        /// Object identifier of server authentication.
        const SERVER_AUTHENTICATION: &str = "1.3.6.1.5.5.7.3.1";

        let mut records = Vec::new();
        let Ok(distrusted) = CertStore::open_current_user("Disallowed") else {
            return Err(refusal());
        };
        let denied: Vec<Vec<u8>> =
            distrusted.certs().map(|certificate| certificate.to_der().to_vec()).collect();
        let Ok(roots) = CertStore::open_current_user("ROOT") else {
            return Err(refusal());
        };
        for certificate in roots.certs() {
            let der = certificate.to_der().to_vec();
            let decision = if denied.contains(&der) {
                ProviderDecision::Distrusted
            } else {
                match certificate.valid_uses() {
                    Ok(schannel::cert_context::ValidUses::All) => {
                        ProviderDecision::UnconditionallyTrustedForServerAuthentication
                    }
                    Ok(schannel::cert_context::ValidUses::Oids(uses))
                        if uses.iter().any(|use_| use_ == SERVER_AUTHENTICATION) =>
                    {
                        ProviderDecision::UnconditionallyTrustedForServerAuthentication
                    }
                    Ok(_) => ProviderDecision::ExternallyRestricted,
                    Err(_) => ProviderDecision::Unevaluable,
                }
            };
            records.push(ProviderRecord { der, decision });
        }
        Ok(records)
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
impl PlatformTrustSource for OperatingSystemTrustSource {
    fn records(&self) -> Result<Vec<ProviderRecord>, ConfigurationDiagnostic> {
        Err(ConfigurationDiagnostic::once(
            DiagnosticSourceClass::PlatformTrust,
            DiagnosticStage::SnapshotConstruction,
            LOCATION,
            ConfigurationFailureCode::UnsupportedPlatform,
        ))
    }
}
