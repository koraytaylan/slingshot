//! Optional certificate authorities that extend author trust.
//!
//! An operator whose Adobe Experience Manager author sits behind a corporate
//! authority needs Slingshot to trust that authority. What they must not be
//! able to do - even by accident, even with a file they own - is make Slingshot
//! trust it for Adobe Identity Management Services, because that is where the
//! credentials go. The type produced here says so in its name: it is an author
//! extension, and no identity-management builder accepts one.
//!
//! Parsing is deliberately strict rather than forgiving. Only `CERTIFICATE`
//! blocks are read, so a file that also carries a private key is refused rather
//! than partly used; the base64 must be exact; and each certificate must be a
//! certificate authority that is allowed to sign certificates and, where it
//! says anything about purpose, allowed to authenticate a server. A file that
//! has to be interpreted charitably is a file whose meaning was never agreed.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use slingshot_domain::profile_authentication_contract::{
    ConfigurationFailureCode, ProfileAuthenticationContract,
};
use x509_parser::prelude::{FromDer, X509Certificate};

use crate::profile_loader::{ConfigurationDiagnostic, DiagnosticSourceClass, DiagnosticStage};

/// Opening of one privacy-enhanced-mail block.
const BLOCK_OPENING: &str = "-----BEGIN ";

/// Closing of one privacy-enhanced-mail block.
const BLOCK_CLOSING: &str = "-----END ";

/// Suffix of both block boundaries.
const BOUNDARY_SUFFIX: &str = "-----";

/// The one label this source may carry.
const CERTIFICATE_LABEL: &str = "CERTIFICATE";

/// Bytes that may appear between two blocks.
const SEPARATING_BYTES: &[char] = &[' ', '\t', '\r', '\n'];

/// Object identifier of server authentication.
const SERVER_AUTHENTICATION: &str = "1.3.6.1.5.5.7.3.1";

/// Structural location every decision here is reported at.
const LOCATION: &str = "additional_ca_certificate_file";

/// Certificates that extend author trust and reach nothing else.
///
/// The type is the boundary. An identity-management client builder accepts a
/// platform snapshot and nothing else, so a value of this type cannot reach it
/// however it is passed around.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdditionalAuthorCertificates {
    /// Distinct certificates, in the order the source listed them.
    certificates: Vec<Vec<u8>>,
}

impl AdditionalAuthorCertificates {
    /// Parses one certificate-authority source.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigurationFailureCode::AdditionalCertificateAuthorityPrivateKey`]
    /// when the source carries any block that is not a certificate,
    /// [`ConfigurationFailureCode::AdditionalCertificateAuthorityLimitExceeded`]
    /// when it carries more certificates or more bytes than the contract
    /// allows, and
    /// [`ConfigurationFailureCode::AdditionalCertificateAuthorityInvalid`] for
    /// an empty source, a malformed block, a duplicate certificate, or a
    /// certificate that is not an eligible authority. No reference, subject,
    /// certificate byte, or parser message survives.
    pub fn parse(source: &[u8]) -> Result<Self, ConfigurationDiagnostic> {
        let contract = ProfileAuthenticationContract::embedded();
        let limits = &contract.limits;
        if u64::try_from(source.len()).unwrap_or(u64::MAX)
            > limits.maximum_additional_certificate_authority_document_bytes
        {
            return Err(refusal(
                ConfigurationFailureCode::AdditionalCertificateAuthorityLimitExceeded,
            ));
        }
        let text = core::str::from_utf8(source).map_err(|_| {
            refusal(ConfigurationFailureCode::AdditionalCertificateAuthorityInvalid)
        })?;
        let blocks = read_blocks(text)?;
        if blocks.is_empty() {
            return Err(refusal(ConfigurationFailureCode::AdditionalCertificateAuthorityInvalid));
        }
        if u64::try_from(blocks.len()).unwrap_or(u64::MAX)
            > limits.maximum_additional_certificate_authorities
        {
            return Err(refusal(
                ConfigurationFailureCode::AdditionalCertificateAuthorityLimitExceeded,
            ));
        }
        let mut certificates: Vec<Vec<u8>> = Vec::with_capacity(blocks.len());
        for block in blocks {
            if u64::try_from(block.len()).unwrap_or(u64::MAX)
                > limits.maximum_additional_certificate_authority_der_bytes
            {
                return Err(refusal(
                    ConfigurationFailureCode::AdditionalCertificateAuthorityLimitExceeded,
                ));
            }
            require_eligible_authority(&block)?;
            if certificates.contains(&block) {
                return Err(refusal(
                    ConfigurationFailureCode::AdditionalCertificateAuthorityInvalid,
                ));
            }
            certificates.push(block);
        }
        Ok(Self { certificates })
    }

    /// Returns the certificates, in the order the source listed them.
    #[must_use]
    pub fn certificates(&self) -> &[Vec<u8>] {
        &self.certificates
    }
}

/// Reads every certificate block, refusing a source that carries anything else.
fn read_blocks(text: &str) -> Result<Vec<Vec<u8>>, ConfigurationDiagnostic> {
    let invalid = || refusal(ConfigurationFailureCode::AdditionalCertificateAuthorityInvalid);
    let mut blocks = Vec::new();
    let mut remainder = text;
    while let Some(opening) = remainder.find(BLOCK_OPENING) {
        if remainder[..opening].contains(|character| !SEPARATING_BYTES.contains(&character)) {
            return Err(invalid());
        }
        let after = &remainder[opening + BLOCK_OPENING.len()..];
        let label_end = after.find(BOUNDARY_SUFFIX).ok_or_else(invalid)?;
        let label = &after[..label_end];
        if label != CERTIFICATE_LABEL {
            return Err(refusal(
                ConfigurationFailureCode::AdditionalCertificateAuthorityPrivateKey,
            ));
        }
        let body = &after[label_end + BOUNDARY_SUFFIX.len()..];
        let closing = format!("{BLOCK_CLOSING}{label}{BOUNDARY_SUFFIX}");
        let body_end = body.find(&closing).ok_or_else(invalid)?;
        let encoded: String = body[..body_end]
            .chars()
            .filter(|character| !SEPARATING_BYTES.contains(character))
            .collect();
        blocks.push(STANDARD.decode(encoded.as_bytes()).map_err(|_| invalid())?);
        remainder = &body[body_end + closing.len()..];
    }
    if remainder.contains(|character| !SEPARATING_BYTES.contains(&character)) {
        return Err(invalid());
    }
    Ok(blocks)
}

/// Requires one certificate to be an authority this route may trust.
///
/// A certificate that says nothing about its purpose is accepted, because an
/// authority predating the extension is still an authority. One that does say
/// something must say it may sign certificates and authenticate a server.
fn require_eligible_authority(der: &[u8]) -> Result<(), ConfigurationDiagnostic> {
    let invalid = || refusal(ConfigurationFailureCode::AdditionalCertificateAuthorityInvalid);
    let (remainder, certificate) = X509Certificate::from_der(der).map_err(|_| invalid())?;
    if !remainder.is_empty() {
        return Err(invalid());
    }
    let authority = certificate
        .basic_constraints()
        .map_err(|_| invalid())?
        .is_some_and(|extension| extension.value.ca);
    if !authority {
        return Err(invalid());
    }
    let signs_certificates = certificate
        .key_usage()
        .map_err(|_| invalid())?
        .is_none_or(|extension| extension.value.key_cert_sign());
    if !signs_certificates {
        return Err(invalid());
    }
    let authenticates_servers =
        certificate.extended_key_usage().map_err(|_| invalid())?.is_none_or(|extension| {
            extension.value.server_auth
                || extension.value.any
                || extension
                    .value
                    .other
                    .iter()
                    .any(|identifier| identifier.to_id_string() == SERVER_AUTHENTICATION)
        });
    if !authenticates_servers {
        return Err(invalid());
    }
    Ok(())
}

/// Returns the one diagnostic this source's failures report.
fn refusal(code: ConfigurationFailureCode) -> ConfigurationDiagnostic {
    ConfigurationDiagnostic::once(
        DiagnosticSourceClass::AdditionalCertificateAuthority,
        DiagnosticStage::DocumentSemantics,
        LOCATION,
        code,
    )
}
