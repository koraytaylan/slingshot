//! Parsing of Adobe Experience Manager Developer Console service credentials.
//!
//! The document this reads is downloaded from the Adobe Experience Manager
//! Developer Console and holds a private key and a client secret. Two things
//! follow from that.
//!
//! The first is that nothing it contains may reach a diagnostic. An unknown key
//! is source bytes, a wrongly typed value is source bytes, and a parser's own
//! message quotes both - so every failure here becomes the same closed tuple,
//! and two documents that fail for different secret-bearing reasons are
//! indistinguishable from outside.
//!
//! The second is depth. A deeply nested document costs work before anything is
//! known about it, so depth is charged from the raw bytes before the shape is
//! interpreted at all: a scalar and an empty container are one level, a nonempty
//! container is one more than its deepest child, and a member name adds nothing.
//!
//! One distinction in this document is easy to get wrong and expensive to get
//! wrong. `technicalAccount.clientId` is the client identity, used for the form
//! field and the assertion audience. `integration.id` is the technical account
//! itself, and it alone becomes the assertion subject and the principal's
//! technical-account identifier. They are different values with different
//! bounds, and swapping them would authenticate as somebody else.

use serde::Deserialize;
use slingshot_configuration::profile_loader::{
    ConfigurationDiagnostic, DiagnosticSourceClass, DiagnosticStage,
};
use slingshot_domain::profile_authentication_contract::{
    ConfigurationFailureCode, ProfileAuthenticationContract,
};
use slingshot_domain::secret_value::{SecretValue, SensitiveConfigurationDocument};
use slingshot_domain::selected_environment_revision::{
    AuthenticationPrincipalIdentity, CanonicalMetascopeSet,
};
use x509_parser::prelude::{FromDer, X509Certificate};

/// Structural location every decision here is reported at.
const LOCATION: &str = "service_credentials";

/// Separator between two metascopes.
const METASCOPE_SEPARATOR: char = ',';

/// Opening of one privacy-enhanced-mail block.
const BLOCK_OPENING: &str = "-----BEGIN ";

/// Closing of one privacy-enhanced-mail block.
const BLOCK_CLOSING: &str = "-----END ";

/// Suffix of both block boundaries.
const BOUNDARY_SUFFIX: &str = "-----";

/// Label of a certificate block.
const CERTIFICATE_LABEL: &str = "CERTIFICATE";

/// Labels a private-key block may carry.
const PRIVATE_KEY_LABELS: &[&str] = &["RSA PRIVATE KEY", "PRIVATE KEY"];

/// Bits in one byte, for reading a modulus width.
const BITS_PER_BYTE: u64 = 8;

/// Tag of an abstract-syntax sequence.
const SEQUENCE_TAG: u8 = 0x30;

/// Tag of an abstract-syntax integer.
const INTEGER_TAG: u8 = 0x02;

/// Tag of an abstract-syntax octet string.
const OCTET_STRING_TAG: u8 = 0x04;

/// Bit that marks a length as spanning several bytes.
const LONG_LENGTH: u8 = 0x80;

/// The credentials one selected environment authenticates with.
///
/// Its rendering carries only the opaque principal digest, because every other
/// field is either a secret or a raw principal value that must not travel.
pub struct CloudServiceCredentials {
    /// Opaque identity of the principal these credentials name.
    principal: AuthenticationPrincipalIdentity,
    /// Canonical authorization scope.
    metascopes: CanonicalMetascopeSet,
    /// Bare authority the document named, kept for policy checking.
    identity_management_authority: String,
    /// Client identity, which the form field and the audience use.
    technical_account_client_identifier: String,
    /// Technical account itself, which the assertion subject uses.
    technical_account_identifier: String,
    /// Organization the technical account belongs to.
    organization_identifier: String,
    /// Client secret the exchange sends.
    client_secret: SecretValue,
    /// Private key the assertion is signed with.
    private_key: SecretValue,
    /// Certificate whose key the private key must match.
    public_certificate: Vec<u8>,
}

impl ::core::fmt::Debug for CloudServiceCredentials {
    fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        formatter
            .debug_struct("CloudServiceCredentials")
            .field("principal", &self.principal.to_string())
            .finish_non_exhaustive()
    }
}

impl CloudServiceCredentials {
    /// Parses one selected service-credential document.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigurationFailureCode::ServiceCredentialsDeprecatedProduct`]
    /// for a document shaped like the deprecated Adobe Developer Console
    /// credential, [`ConfigurationFailureCode::ServiceCredentialsKeyMismatch`]
    /// when the private key does not match the certificate,
    /// [`ConfigurationFailureCode::IdentityManagementAuthorityNotAllowed`] for
    /// any other authority, and
    /// [`ConfigurationFailureCode::ServiceCredentialsInvalid`] for everything
    /// else. No failure carries a byte of the document.
    pub fn parse(
        document: &SensitiveConfigurationDocument,
    ) -> Result<Self, ConfigurationDiagnostic> {
        let contract = ProfileAuthenticationContract::embedded();
        let limits = &contract.limits;
        if u64::try_from(document.document_byte_length()).unwrap_or(u64::MAX)
            > limits.maximum_service_credential_document_bytes
        {
            return Err(refusal(ConfigurationFailureCode::ServiceCredentialsInvalid));
        }
        let text = document
            .lend_text_for_parsing(str::to_owned)
            .map_err(|_| refusal(ConfigurationFailureCode::ServiceCredentialsInvalid))?;
        charge_depth(&text, limits.maximum_service_credential_json_depth)?;
        let parsed: CredentialDocument =
            serde_json::from_str(&text).map_err(|_| classify_shape(&text))?;
        Self::build(parsed)
    }

    /// Returns the opaque identity of the principal these credentials name.
    #[must_use]
    pub fn principal(&self) -> AuthenticationPrincipalIdentity {
        self.principal
    }

    /// Returns the canonical authorization scope.
    #[must_use]
    pub fn metascopes(&self) -> &CanonicalMetascopeSet {
        &self.metascopes
    }

    /// Returns the bare authority the document named.
    #[must_use]
    pub fn identity_management_authority(&self) -> &str {
        &self.identity_management_authority
    }

    /// Returns the client identity the form field and the audience use.
    #[must_use]
    pub fn technical_account_client_identifier(&self) -> &str {
        &self.technical_account_client_identifier
    }

    /// Returns the technical account the assertion subject names.
    #[must_use]
    pub fn technical_account_identifier(&self) -> &str {
        &self.technical_account_identifier
    }

    /// Returns the organization the assertion issuer names.
    #[must_use]
    pub fn organization_identifier(&self) -> &str {
        &self.organization_identifier
    }

    /// Returns the client secret the exchange sends.
    #[must_use]
    pub fn client_secret(&self) -> &SecretValue {
        &self.client_secret
    }

    /// Returns the private key the assertion is signed with.
    #[must_use]
    pub fn private_key(&self) -> &SecretValue {
        &self.private_key
    }

    /// Returns the certificate whose key the private key matches.
    #[must_use]
    pub fn public_certificate(&self) -> &[u8] {
        &self.public_certificate
    }

    /// Validates one parsed document and builds the credentials.
    fn build(document: CredentialDocument) -> Result<Self, ConfigurationDiagnostic> {
        let contract = ProfileAuthenticationContract::embedded();
        let limits = &contract.limits;
        let literals = &contract.literals;
        let invalid = || refusal(ConfigurationFailureCode::ServiceCredentialsInvalid);
        if !document.ok || document.status_code != limits.service_credential_status_code {
            return Err(invalid());
        }
        let integration = document.integration;
        if !literals
            .identity_management_authorities
            .contains(&integration.identity_management_endpoint)
        {
            return Err(refusal(ConfigurationFailureCode::IdentityManagementAuthorityNotAllowed));
        }
        bounded(
            &integration.organization_identifier,
            limits.maximum_organization_identifier_bytes,
        )?;
        bounded(
            &integration.technical_account_identifier,
            limits.maximum_technical_account_identifier_bytes,
        )?;
        bounded(&integration.email, limits.maximum_technical_account_email_bytes)?;
        bounded(
            &integration.technical_account.client_identifier,
            limits.maximum_technical_account_client_identifier_bytes,
        )?;
        bounded(
            &integration.technical_account.client_secret,
            limits.maximum_technical_account_client_secret_bytes,
        )?;
        bounded(&integration.private_key, limits.maximum_private_key_pem_bytes)?;
        bounded(&integration.public_key, limits.maximum_public_certificate_pem_bytes)?;
        require_unreserved(&integration.technical_account.client_identifier)?;
        require_no_control(&integration.organization_identifier)?;
        require_no_control(&integration.technical_account_identifier)?;
        require_no_control(&integration.email)?;
        let metascopes = canonical_metascopes(&integration.metascopes)?;
        let certificate = read_single_block(&integration.public_key, CERTIFICATE_LABEL)?;
        let key = read_private_key(&integration.private_key)?;
        require_matching_key(&key, &certificate)?;
        let principal = AuthenticationPrincipalIdentity::cloud(
            &literals.authentication_methods[1],
            &integration.organization_identifier,
            &integration.technical_account.client_identifier,
            &integration.technical_account_identifier,
        )
        .map_err(|_| invalid())?;
        Ok(Self {
            principal,
            metascopes,
            identity_management_authority: integration.identity_management_endpoint,
            technical_account_client_identifier: integration.technical_account.client_identifier,
            technical_account_identifier: integration.technical_account_identifier,
            organization_identifier: integration.organization_identifier,
            client_secret: SecretValue::from_text(integration.technical_account.client_secret),
            private_key: SecretValue::from_text(integration.private_key),
            public_certificate: certificate,
        })
    }
}

/// The downloaded document exactly as it is spelled.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CredentialDocument {
    ok: bool,
    #[serde(rename = "statusCode")]
    status_code: u64,
    integration: IntegrationDocument,
}

/// Its integration object exactly as it is spelled.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct IntegrationDocument {
    #[serde(rename = "imsEndpoint")]
    identity_management_endpoint: String,
    metascopes: String,
    #[serde(rename = "technicalAccount")]
    technical_account: TechnicalAccountDocument,
    email: String,
    #[serde(rename = "id")]
    technical_account_identifier: String,
    #[serde(rename = "org")]
    organization_identifier: String,
    #[serde(rename = "privateKey")]
    private_key: String,
    #[serde(rename = "publicKey")]
    public_key: String,
}

/// Its technical-account object exactly as it is spelled.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TechnicalAccountDocument {
    #[serde(rename = "clientId")]
    client_identifier: String,
    #[serde(rename = "clientSecret")]
    client_secret: String,
}

/// Charges depth from the raw bytes, before any shape is interpreted.
///
/// A scalar and an empty container are one level; a nonempty container is one
/// more than its deepest child; a member name adds nothing. Arrays and objects
/// charge identically, so nesting cannot be made cheaper by choosing one.
///
/// The count is taken two ways at once. An opening bracket is itself a level,
/// which is what makes an empty container cost one. And any value inside a
/// container is one level deeper than the container it sits in, which is what
/// makes a scalar leaf cost the level it is written at. A member name is a
/// string at the same level as the value beside it, so counting it changes
/// nothing.
fn charge_depth(text: &str, ceiling: u64) -> Result<(), ConfigurationDiagnostic> {
    let mut open: u64 = 0;
    let mut deepest: u64 = 1;
    let mut reader = StringReader::default();
    for byte in text.bytes() {
        if reader.consumed(byte) {
            continue;
        }
        match byte {
            b'{' | b'[' => {
                open = open.saturating_add(1);
                deepest = deepest.max(open);
            }
            b'}' | b']' => open = open.saturating_sub(1),
            _ if is_structural(byte) => {}
            _ => {
                reader.opened(byte);
                deepest = deepest.max(open.saturating_add(1));
            }
        }
        if deepest > ceiling {
            return Err(refusal(ConfigurationFailureCode::ServiceCredentialsInvalid));
        }
    }
    Ok(())
}

/// Reports whether one byte separates values rather than being one.
fn is_structural(byte: u8) -> bool {
    byte == b',' || byte == b':' || byte.is_ascii_whitespace()
}

/// Tracks whether the reader is inside a string, so a bracket written in one
/// does not charge a level.
#[derive(Debug, Default)]
struct StringReader {
    /// Whether a string is open.
    inside: bool,
    /// Whether the previous byte began an escape.
    escaped: bool,
}

impl StringReader {
    /// Consumes one byte when a string is open, reporting that it did.
    fn consumed(&mut self, byte: u8) -> bool {
        if !self.inside {
            return false;
        }
        match byte {
            _ if self.escaped => self.escaped = false,
            b'\\' => self.escaped = true,
            b'"' => self.inside = false,
            _ => {}
        }
        true
    }

    /// Notes that one byte outside a string may have opened one.
    fn opened(&mut self, byte: u8) {
        self.inside = byte == b'"';
    }
}

/// Returns the failure one unusable document shape reports.
///
/// A document carrying the members the deprecated Adobe Developer Console
/// credential carries is named as that product, because an operator who
/// downloaded the wrong file needs to know which one to download instead. The
/// judgement uses member names this code already knows, never bytes from the
/// document.
fn classify_shape(text: &str) -> ConfigurationDiagnostic {
    let deprecated = ["\"client_id\"", "\"client_secret\"", "\"technical_account_id\""];
    if deprecated.iter().all(|member| text.contains(member)) {
        return refusal(ConfigurationFailureCode::ServiceCredentialsDeprecatedProduct);
    }
    refusal(ConfigurationFailureCode::ServiceCredentialsInvalid)
}

/// Requires one value to be nonempty and within its bound.
fn bounded(value: &str, maximum_bytes: u64) -> Result<(), ConfigurationDiagnostic> {
    let length = u64::try_from(value.len()).unwrap_or(u64::MAX);
    if value.is_empty() || length > maximum_bytes {
        return Err(refusal(ConfigurationFailureCode::ServiceCredentialsInvalid));
    }
    Ok(())
}

/// Requires one value to be the unreserved bytes a client identity may use.
fn require_unreserved(value: &str) -> Result<(), ConfigurationDiagnostic> {
    let usable =
        value.bytes().all(|byte| byte.is_ascii_alphanumeric() || "-._~".contains(char::from(byte)));
    if usable {
        return Ok(());
    }
    Err(refusal(ConfigurationFailureCode::ServiceCredentialsInvalid))
}

/// Requires one value to carry no control character.
///
/// The bytes receive no Unicode normalization, because the principal identity
/// is derived from them exactly as the document spelled them.
fn require_no_control(value: &str) -> Result<(), ConfigurationDiagnostic> {
    if value.chars().any(char::is_control) {
        return Err(refusal(ConfigurationFailureCode::ServiceCredentialsInvalid));
    }
    Ok(())
}

/// Returns the canonical form of the comma-separated metascope input.
fn canonical_metascopes(input: &str) -> Result<CanonicalMetascopeSet, ConfigurationDiagnostic> {
    let limits = &ProfileAuthenticationContract::embedded().limits;
    let invalid = || refusal(ConfigurationFailureCode::ServiceCredentialsInvalid);
    bounded(input, limits.maximum_metascopes_bytes)?;
    let values: Vec<&str> = input.split(METASCOPE_SEPARATOR).collect();
    if u64::try_from(values.len()).unwrap_or(u64::MAX) > limits.maximum_metascopes {
        return Err(invalid());
    }
    let mut seen: Vec<String> = Vec::with_capacity(values.len());
    for value in values {
        let length = u64::try_from(value.len()).unwrap_or(u64::MAX);
        let usable = !value.is_empty()
            && length <= limits.maximum_metascope_bytes
            && value
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_');
        if !usable || seen.iter().any(|known| known == value) {
            return Err(invalid());
        }
        seen.push(value.to_owned());
    }
    Ok(CanonicalMetascopeSet::from_values(&seen))
}

/// Reads exactly one privacy-enhanced-mail block carrying `label`.
fn read_single_block(text: &str, label: &str) -> Result<Vec<u8>, ConfigurationDiagnostic> {
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD;

    let invalid = || refusal(ConfigurationFailureCode::ServiceCredentialsInvalid);
    let opening = format!("{BLOCK_OPENING}{label}{BOUNDARY_SUFFIX}");
    let closing = format!("{BLOCK_CLOSING}{label}{BOUNDARY_SUFFIX}");
    let start = text.find(&opening).ok_or_else(invalid)? + opening.len();
    let end = text[start..].find(&closing).ok_or_else(invalid)? + start;
    if text[end + closing.len()..].contains(BLOCK_OPENING) {
        return Err(invalid());
    }
    let encoded: String = text[start..end].chars().filter(|byte| !byte.is_whitespace()).collect();
    STANDARD.decode(encoded.as_bytes()).map_err(|_| invalid())
}

/// One decoded key's public parameters.
#[derive(Debug, PartialEq, Eq)]
struct KeyParameters {
    /// Modulus of the key, without its leading sign byte.
    modulus: Vec<u8>,
    /// Public exponent of the key.
    exponent: u64,
}

/// Reads the public parameters of the document's private key.
fn read_private_key(text: &str) -> Result<KeyParameters, ConfigurationDiagnostic> {
    let invalid = || refusal(ConfigurationFailureCode::ServiceCredentialsInvalid);
    let label = PRIVATE_KEY_LABELS
        .iter()
        .find(|label| text.contains(&format!("{BLOCK_OPENING}{label}{BOUNDARY_SUFFIX}")))
        .ok_or_else(invalid)?;
    let der = read_single_block(text, label)?;
    let sequence = read_element(&der, SEQUENCE_TAG).ok_or_else(invalid)?;
    let (_, after_version) = read_integer(sequence).ok_or_else(invalid)?;
    if let Some(parameters) = read_pkcs_one(after_version) {
        return validate_parameters(parameters);
    }
    let wrapped = find_octet_string(sequence).ok_or_else(invalid)?;
    let inner = read_element(wrapped, SEQUENCE_TAG).ok_or_else(invalid)?;
    let (_, after_inner_version) = read_integer(inner).ok_or_else(invalid)?;
    validate_parameters(read_pkcs_one(after_inner_version).ok_or_else(invalid)?)
}

/// Reads the modulus and exponent of a key structure.
///
/// A private key writes them after its version and a public key writes them
/// first, so the caller positions the reader and this reads the same pair.
fn read_pkcs_one(body: &[u8]) -> Option<KeyParameters> {
    let (modulus, remainder) = read_integer(body)?;
    let (exponent, _) = read_integer(remainder)?;
    let exponent = exponent.iter().try_fold(0_u64, |total, byte| {
        total.checked_mul(u64::from(u8::MAX) + 1)?.checked_add(u64::from(*byte))
    })?;
    Some(KeyParameters { modulus: strip_sign(modulus).to_vec(), exponent })
}

/// Requires the key's width and exponent to be ones the contract accepts.
fn validate_parameters(
    parameters: KeyParameters,
) -> Result<KeyParameters, ConfigurationDiagnostic> {
    let limits = &ProfileAuthenticationContract::embedded().limits;
    let bits = u64::try_from(parameters.modulus.len()).unwrap_or(u64::MAX) * BITS_PER_BYTE;
    let usable = bits >= limits.minimum_service_credential_rsa_modulus_bits
        && bits <= limits.maximum_service_credential_rsa_modulus_bits
        && parameters.exponent == limits.service_credential_rsa_public_exponent;
    if usable {
        return Ok(parameters);
    }
    Err(refusal(ConfigurationFailureCode::ServiceCredentialsInvalid))
}

/// Requires the certificate's key to be the private key's public half.
fn require_matching_key(
    key: &KeyParameters,
    certificate: &[u8],
) -> Result<(), ConfigurationDiagnostic> {
    let mismatch = || refusal(ConfigurationFailureCode::ServiceCredentialsKeyMismatch);
    let (remainder, parsed) = X509Certificate::from_der(certificate).map_err(|_| mismatch())?;
    if !remainder.is_empty() {
        return Err(mismatch());
    }
    let public = parsed.public_key().subject_public_key.data.as_ref();
    let sequence = read_element(public, SEQUENCE_TAG).ok_or_else(mismatch)?;
    let announced = read_pkcs_one(sequence).ok_or_else(mismatch)?;
    if &announced == key {
        return Ok(());
    }
    Err(mismatch())
}

/// Returns the body of one element carrying `tag`.
fn read_element(bytes: &[u8], tag: u8) -> Option<&[u8]> {
    let (element, _) = read_tagged(bytes, tag)?;
    Some(element)
}

/// Returns the first integer of `bytes` and whatever follows it.
fn read_integer(bytes: &[u8]) -> Option<(&[u8], &[u8])> {
    read_tagged(bytes, INTEGER_TAG)
}

/// Returns the first octet string anywhere in one sequence body.
fn find_octet_string(bytes: &[u8]) -> Option<&[u8]> {
    let mut remainder = bytes;
    while !remainder.is_empty() {
        if remainder[0] == OCTET_STRING_TAG {
            return read_tagged(remainder, OCTET_STRING_TAG).map(|(body, _)| body);
        }
        let (_, rest) = read_tagged(remainder, remainder[0])?;
        remainder = rest;
    }
    None
}

/// Returns the body of the leading element when it carries `tag`.
fn read_tagged(bytes: &[u8], tag: u8) -> Option<(&[u8], &[u8])> {
    if bytes.first() != Some(&tag) {
        return None;
    }
    /// Bytes an element's tag and its first length byte occupy.
    const SHORT_HEADER: usize = 2;

    let first = *bytes.get(1)?;
    let (length, header) = if first & LONG_LENGTH == 0 {
        (usize::from(first), SHORT_HEADER)
    } else {
        let count = usize::from(first & !LONG_LENGTH);
        let digits = bytes.get(SHORT_HEADER..SHORT_HEADER.checked_add(count)?)?;
        let length = digits.iter().try_fold(0_usize, |total, byte| {
            total.checked_mul(usize::from(u8::MAX) + 1)?.checked_add(usize::from(*byte))
        })?;
        (length, SHORT_HEADER.checked_add(count)?)
    };
    let body = bytes.get(header..header.checked_add(length)?)?;
    Some((body, bytes.get(header + length..)?))
}

/// Removes the leading zero an unsigned integer carries to stay positive.
fn strip_sign(integer: &[u8]) -> &[u8] {
    match integer.split_first() {
        Some((0, remainder)) => remainder,
        _ => integer,
    }
}

/// Returns the one diagnostic a credential failure reports.
fn refusal(code: ConfigurationFailureCode) -> ConfigurationDiagnostic {
    ConfigurationDiagnostic::once(
        DiagnosticSourceClass::ServiceCredentials,
        DiagnosticStage::DocumentSemantics,
        LOCATION,
        code,
    )
}
