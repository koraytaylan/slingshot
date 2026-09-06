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
//! bytes says the same thing. A conflicting duplicate fails the snapshot rather
//! than being resolved, because resolving it would mean choosing which of the
//! platform's two answers to believe.
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

/// The immutable platform roots one startup accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformTrustSnapshot {
    /// Distinct retained roots, in ascending byte order.
    roots: Vec<Vec<u8>>,
}

impl PlatformTrustSnapshot {
    /// Takes one snapshot from `source`.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigurationFailureCode::PlatformTrustSnapshotInvalid`] when
    /// the store cannot be enumerated, holds a record whose decision is not
    /// unconditional, holds two records for the same bytes that disagree, or
    /// exceeds the contract's count, entry, or aggregate bounds. No record byte,
    /// subject, or provider message survives.
    pub fn take(source: &dyn PlatformTrustSource) -> Result<Self, ConfigurationDiagnostic> {
        let limits = &ProfileAuthenticationContract::embedded().limits;
        let records = source.records()?;
        let mut decided: Vec<(Vec<u8>, ProviderDecision)> = Vec::new();
        for record in records {
            if u64::try_from(record.der.len()).unwrap_or(u64::MAX)
                > limits.maximum_platform_trust_authority_der_bytes
            {
                return Err(refusal());
            }
            match decided.iter().find(|(der, _)| *der == record.der) {
                Some((_, decision)) if *decision != record.decision => return Err(refusal()),
                Some(_) => continue,
                None => decided.push((record.der, record.decision)),
            }
        }
        let mut roots = Vec::new();
        for (der, decision) in decided {
            if decision != ProviderDecision::UnconditionallyTrustedForServerAuthentication {
                return Err(refusal());
            }
            require_eligible_anchor(&der)?;
            roots.push(der);
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
        Ok(Self { roots })
    }

    /// Returns the retained roots, in ascending byte order.
    #[must_use]
    pub fn roots(&self) -> &[Vec<u8>] {
        &self.roots
    }
}

/// Requires one retained anchor to be an authority that may authenticate a
/// server, so accepted bytes fully represent the decision they came with.
fn require_eligible_anchor(der: &[u8]) -> Result<(), ConfigurationDiagnostic> {
    let (remainder, certificate) = X509Certificate::from_der(der).map_err(|_| refusal())?;
    if !remainder.is_empty() {
        return Err(refusal());
    }
    let authority = certificate
        .basic_constraints()
        .map_err(|_| refusal())?
        .is_some_and(|extension| extension.value.ca);
    if !authority {
        return Err(refusal());
    }
    let authenticates_servers = certificate
        .extended_key_usage()
        .map_err(|_| refusal())?
        .is_none_or(|extension| extension.value.server_auth || extension.value.any);
    if authenticates_servers {
        return Ok(());
    }
    Err(refusal())
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
            read_bundle(&path, |source| {
                for der in parse_platform_bundle(source)? {
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
    let limits = &ProfileAuthenticationContract::embedded().limits;
    if source.len() as u64
        > limits.maximum_identity_management_trust_canonical_bytes.saturating_mul(2)
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
        require_eligible_anchor(der)?;
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
    let maximum = limits.maximum_identity_management_trust_canonical_bytes.saturating_mul(2);
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
        if index as u64 >= limits.maximum_platform_trust_authorities.saturating_mul(4) {
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
        if bytes > maximum.saturating_mul(4) {
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
