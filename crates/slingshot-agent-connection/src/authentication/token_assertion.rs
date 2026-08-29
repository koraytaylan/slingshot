//! Construction of one signed service-credential assertion.
//!
//! The assertion is the only thing Adobe Identity Management Services sees of
//! the private key, so its bytes are pinned rather than described. Two
//! implementations that agree on the claims can still disagree on the bytes -
//! one escapes a solidus, one orders members differently, one pads its base64 -
//! and a signature is over bytes, not over claims. Everything here is therefore
//! written out exactly: member order, string escaping, integer form, segment
//! encoding, and framing.
//!
//! The clock is sampled once and injected. Sampling it twice would let the
//! validity check and the expiry disagree; reading a system clock directly would
//! make every boundary case untestable.
//!
//! Nothing a credential carries can add to the assertion. The audience and each
//! metascope claim are built from the validated authority and validated values,
//! so no input can introduce a path, a query, a header parameter, or a claim of
//! its own.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use slingshot_domain::profile_authentication_contract::{
    ConfigurationFailureCode, ProfileAuthenticationContract,
};
use slingshot_domain::secret_value::SecretValue;
use x509_parser::prelude::{FromDer, X509Certificate};

use crate::authentication::cloud_service_credentials::CloudServiceCredentials;

/// Separator between two compact segments.
const SEGMENT_SEPARATOR: char = '.';

/// Claim naming the audience.
const AUDIENCE_CLAIM: &str = "aud";

/// Claim naming the expiry.
const EXPIRY_CLAIM: &str = "exp";

/// Claim naming the issuer.
const ISSUER_CLAIM: &str = "iss";

/// Claim naming the subject.
const SUBJECT_CLAIM: &str = "sub";

/// Reason one assertion could not be built.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("{code}")]
pub struct AssertionFailure {
    /// Stable code from the contract registry.
    pub code: ConfigurationFailureCode,
}

impl AssertionFailure {
    /// Returns one failure carrying `code`.
    #[must_use]
    pub fn new(code: ConfigurationFailureCode) -> Self {
        Self { code }
    }
}

/// Answers what time it is, once.
///
/// The trait exists so every boundary - an unavailable clock, a second outside
/// the accepted range, a certificate that is not yet valid, one that has
/// expired, and equality at either edge - is provable without waiting for it.
pub trait CoordinatedUniversalTimeClock {
    /// Returns the current whole Unix second, when the clock has one.
    fn sample(&self) -> Option<u64>;
}

/// One assertion, ready to be exchanged.
///
/// It is a secret: it is signed with the private key and Adobe accepts it in
/// place of one, so it renders redacted and reaches its bytes only through a
/// call named for that.
#[derive(Debug)]
pub struct ServiceCredentialAssertion {
    /// The compact serialization.
    compact: SecretValue,
    /// The second the clock was sampled at.
    issued_at: u64,
    /// The second the assertion expires at.
    expires_at: u64,
}

impl ServiceCredentialAssertion {
    /// Builds one assertion from validated credentials and one clock sample.
    ///
    /// # Errors
    ///
    /// Returns, in this order,
    /// [`ConfigurationFailureCode::AssertionClockUnavailable`],
    /// [`ConfigurationFailureCode::AssertionClockOutOfRange`],
    /// [`ConfigurationFailureCode::AssertionCertificateNotYetValid`],
    /// [`ConfigurationFailureCode::AssertionCertificateExpired`], and
    /// [`ConfigurationFailureCode::AssertionSigningFailed`]. The order is the
    /// contract's: a signing failure is reported only once the clock and the
    /// certificate have both been accepted.
    pub fn build(
        credentials: &CloudServiceCredentials,
        clock: &dyn CoordinatedUniversalTimeClock,
    ) -> Result<Self, AssertionFailure> {
        let contract = ProfileAuthenticationContract::embedded();
        let limits = &contract.limits;
        let sampled = clock.sample().ok_or_else(|| {
            AssertionFailure::new(ConfigurationFailureCode::AssertionClockUnavailable)
        })?;
        if sampled > limits.maximum_service_credential_utc_unix_seconds {
            return Err(AssertionFailure::new(ConfigurationFailureCode::AssertionClockOutOfRange));
        }
        require_valid_certificate(credentials.public_certificate(), sampled)?;
        let expires_at =
            sampled.checked_add(limits.service_credential_assertion_lifetime_seconds).ok_or_else(
                || AssertionFailure::new(ConfigurationFailureCode::AssertionClockOutOfRange),
            )?;
        let header = contract.literals.assertion_protected_header.as_bytes();
        let payload = build_payload(credentials, expires_at);
        let signing_input =
            format!("{}{SEGMENT_SEPARATOR}{}", encode(header), encode(payload.as_bytes()));
        let signature = sign(&signing_input, credentials.private_key())?;
        let compact = format!("{signing_input}{SEGMENT_SEPARATOR}{signature}");
        if u64::try_from(compact.len()).unwrap_or(u64::MAX)
            > limits.maximum_service_credential_assertion_bytes
        {
            return Err(AssertionFailure::new(ConfigurationFailureCode::AssertionSigningFailed));
        }
        Ok(Self { compact: SecretValue::from_text(compact), issued_at: sampled, expires_at })
    }

    /// Lends the compact serialization to `use_bytes`.
    ///
    /// It is lent rather than returned because it authenticates as its holder,
    /// exactly like the key it was signed with.
    pub fn lend_compact_bytes<Outcome>(&self, use_bytes: impl FnOnce(&[u8]) -> Outcome) -> Outcome {
        use_bytes(self.compact.expose_secret_bytes())
    }

    /// Returns how many bytes the compact serialization occupies.
    #[must_use]
    pub fn compact_byte_length(&self) -> usize {
        self.compact.secret_byte_length()
    }

    /// Returns the second the clock was sampled at.
    #[must_use]
    pub fn issued_at(&self) -> u64 {
        self.issued_at
    }

    /// Returns the second the assertion expires at.
    #[must_use]
    pub fn expires_at(&self) -> u64 {
        self.expires_at
    }
}

/// Requires the sampled second to lie inside the certificate's validity.
///
/// Equality at either boundary is accepted, because a certificate is valid on
/// the second it becomes valid and on the second it stops being so.
fn require_valid_certificate(certificate: &[u8], sampled: u64) -> Result<(), AssertionFailure> {
    let expired = || AssertionFailure::new(ConfigurationFailureCode::AssertionCertificateExpired);
    let (_, parsed) = X509Certificate::from_der(certificate).map_err(|_| expired())?;
    let sampled = i64::try_from(sampled)
        .map_err(|_| AssertionFailure::new(ConfigurationFailureCode::AssertionClockOutOfRange))?;
    if sampled < parsed.validity().not_before.timestamp() {
        return Err(AssertionFailure::new(
            ConfigurationFailureCode::AssertionCertificateNotYetValid,
        ));
    }
    if sampled > parsed.validity().not_after.timestamp() {
        return Err(expired());
    }
    Ok(())
}

/// Builds the exact payload bytes.
///
/// Members are ordered by ascending key bytes and integers are minimal
/// unsigned decimal, because the signature is over these bytes and any other
/// rendering of the same claims is a different assertion.
fn build_payload(credentials: &CloudServiceCredentials, expires_at: u64) -> String {
    let literals = &ProfileAuthenticationContract::embedded().literals;
    let audience = format!(
        "{}{}",
        literals.assertion_audience_prefix,
        credentials.technical_account_client_identifier()
    );
    let mut members = vec![
        (AUDIENCE_CLAIM.to_owned(), render_string(&audience)),
        (EXPIRY_CLAIM.to_owned(), expires_at.to_string()),
        (ISSUER_CLAIM.to_owned(), render_string(credentials.organization_identifier())),
        (SUBJECT_CLAIM.to_owned(), render_string(credentials.technical_account_identifier())),
    ];
    for metascope in credentials.metascopes().values().unwrap_or_default() {
        let claim = format!("{}{metascope}", literals.assertion_metascope_claim_prefix);
        members.push((claim, "true".to_owned()));
    }
    members.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
    let rendered: Vec<String> =
        members.iter().map(|(name, value)| format!("{}:{value}", render_string(name))).collect();
    format!("{{{}}}", rendered.join(","))
}

/// Renders one string exactly as the contract spells it.
///
/// Only the quote and the reverse solidus are escaped. The solidus is never
/// escaped and no character is ever written as a numeric escape, because both
/// would produce a different byte sequence for the same value.
fn render_string(value: &str) -> String {
    /// Quotes one rendered string carries.
    const QUOTES: usize = 2;

    let mut rendered = String::with_capacity(value.len() + QUOTES);
    rendered.push('"');
    for character in value.chars() {
        match character {
            '"' => rendered.push_str("\\\""),
            '\\' => rendered.push_str("\\\\"),
            _ => rendered.push(character),
        }
    }
    rendered.push('"');
    rendered
}

/// Encodes one segment exactly as the contract spells it.
fn encode(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Signs the two-segment input with the credential private key.
fn sign(signing_input: &str, private_key: &SecretValue) -> Result<String, AssertionFailure> {
    let failed = || AssertionFailure::new(ConfigurationFailureCode::AssertionSigningFailed);
    let key = jsonwebtoken::EncodingKey::from_rsa_pem(private_key.expose_secret_bytes())
        .map_err(|_| failed())?;
    jsonwebtoken::crypto::sign(signing_input.as_bytes(), &key, jsonwebtoken::Algorithm::RS256)
        .map_err(|_| failed())
}
