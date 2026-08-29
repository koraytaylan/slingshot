//! Exchange of one assertion for an access token.
//!
//! One request leaves this module, carrying a client secret and a signed
//! assertion to a fixed endpoint. Everything here exists because of what that
//! request is worth to whoever receives it.
//!
//! The endpoint is constructed internally from the manifest, never from the
//! credential, so a credential naming another host cannot redirect the secret it
//! carries. A redirection response is refused without its `Location` being read,
//! because following one would mean sending the secret somewhere the operator
//! never approved - and the fact that a redirection arrived is already knowable
//! only after the secret reached the original endpoint.
//!
//! Every response section is charged before it is interpreted. A head that is
//! too large is refused as too large rather than being parsed to find out what
//! it says, and the first informational head ends the exchange rather than being
//! skipped, because skipping means reading whatever follows.
//!
//! The lease is measured conservatively. The clock is anchored before the first
//! request byte and read again after the stream ends, so every millisecond spent
//! writing, waiting, and reading reduces the lifetime this token is treated as
//! having. A token that would be usable for less than the refresh skew plus the
//! minimum usable lease is not installed at all, and produces no retry, because
//! retrying a short lease immediately is a loop.

use slingshot_domain::profile_authentication_contract::{
    ConfigurationFailureCode, ProfileAuthenticationContract,
};
use slingshot_domain::secret_value::SecretValue;

use crate::authentication::cloud_service_credentials::CloudServiceCredentials;
use crate::authentication::token_assertion::ServiceCredentialAssertion;

/// Field naming the client identity in the request body.
const CLIENT_FIELD: usize = 0;

/// Field naming the client secret in the request body.
const SECRET_FIELD: usize = 1;

/// Field naming the assertion in the request body.
const ASSERTION_FIELD: usize = 2;

/// Field a response uses to declare a trailer section.
const TRAILER_FIELD: &str = "trailer";

/// Field a response uses to declare its media type.
const CONTENT_TYPE_FIELD: &str = "content-type";

/// Field a response uses to declare its content coding.
const CONTENT_ENCODING_FIELD: &str = "content-encoding";

/// Lowest status a redirection uses.
const LOWEST_REDIRECTION: u16 = 300;

/// Statuses one status class spans.
const STATUS_CLASS_SPAN: u16 = 100;

/// Reason one exchange did not produce a token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("{code}")]
pub struct ExchangeFailure {
    /// Stable code from the contract registry.
    pub code: ConfigurationFailureCode,
}

impl ExchangeFailure {
    /// Returns one failure carrying `code`.
    #[must_use]
    pub fn new(code: ConfigurationFailureCode) -> Self {
        Self { code }
    }
}

/// One decoded response section.
///
/// The representation is protocol-independent on purpose: the charges the
/// contract names are over decoded names and values, so the same accounting
/// applies whichever protocol version carried them and compression can never
/// make a section cheaper than it is.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DecodedSection {
    /// Fields the section carries, in the order they arrived.
    pub fields: Vec<(String, String)>,
}

/// One decoded response head.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedHead {
    /// Status the head declares.
    pub status: u16,
    /// Fields the head carries, in the order they arrived.
    pub fields: Vec<(String, String)>,
}

/// One complete decoded response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedResponse {
    /// Informational heads that arrived before the final one.
    pub informational: Vec<DecodedHead>,
    /// The final head.
    pub head: DecodedHead,
    /// The response body.
    pub body: Vec<u8>,
    /// The trailer section, when the response carried one.
    pub trailer: Option<DecodedSection>,
}

/// Carries one exchange request and returns what came back.
///
/// The trait exists so every deadline, every framing failure, and every
/// response shape is provable without a network: a test scripts what the
/// endpoint answered, including answers a real endpoint would rarely give.
pub trait IdentityManagementTransport {
    /// Sends one form body and returns the decoded response.
    ///
    /// # Errors
    ///
    /// Returns the contract code for the phase that failed.
    fn exchange(&self, body: &[u8]) -> Result<DecodedResponse, ExchangeFailure>;
}

/// Answers how much time has passed, in milliseconds.
pub trait MonotonicClock {
    /// Returns the current reading.
    fn reading_milliseconds(&self) -> u64;
}

/// One access token and the moment it stops being usable.
#[derive(Debug)]
pub struct AccessToken {
    /// The token itself.
    token: SecretValue,
    /// Reading of the monotonic clock the token expires at.
    deadline_milliseconds: u64,
}

impl AccessToken {
    /// Lends the token bytes to `use_bytes`.
    pub fn lend_token_bytes<Outcome>(&self, use_bytes: impl FnOnce(&[u8]) -> Outcome) -> Outcome {
        use_bytes(self.token.expose_secret_bytes())
    }

    /// Returns the monotonic reading the token expires at.
    #[must_use]
    pub fn deadline_milliseconds(&self) -> u64 {
        self.deadline_milliseconds
    }

    /// Reports whether the token should be refreshed at `reading`.
    ///
    /// Equality is refresh-required, because a token that expires exactly at
    /// the skew boundary has no usable lease left by the time it is used.
    #[must_use]
    pub fn refresh_required(&self, reading: u64) -> bool {
        let skew =
            ProfileAuthenticationContract::embedded().limits.access_token_refresh_skew_milliseconds;
        self.deadline_milliseconds.saturating_sub(reading) <= skew
    }
}

/// Exchanges one assertion for one access token.
#[derive(Debug)]
pub struct IdentityManagementExchange<Transport, Clock> {
    /// Transport the request is carried over.
    transport: Transport,
    /// Clock the lease is measured with.
    clock: Clock,
}

impl<Transport: IdentityManagementTransport, Clock: MonotonicClock>
    IdentityManagementExchange<Transport, Clock>
{
    /// Returns an exchange over `transport`, measured with `clock`.
    #[must_use]
    pub fn new(transport: Transport, clock: Clock) -> Self {
        Self { transport, clock }
    }

    /// Exchanges one assertion for one access token.
    ///
    /// # Errors
    ///
    /// Returns the contract code of the first checkpoint that refused, in the
    /// manifest's own precedence order.
    pub fn exchange(
        &self,
        credentials: &CloudServiceCredentials,
        assertion: &ServiceCredentialAssertion,
    ) -> Result<AccessToken, ExchangeFailure> {
        let body = build_form_body(credentials, assertion)?;
        let anchor = self.clock.reading_milliseconds();
        let response = self.transport.exchange(&body)?;
        let receipt = self.clock.reading_milliseconds();
        let document = accept_response(&response)?;
        install(document, anchor, receipt)
    }
}

/// One accepted response document.
#[derive(Debug, Clone, PartialEq, Eq)]
struct TokenDocument {
    /// The token itself.
    access_token: String,
    /// The lifetime the response advertised, in milliseconds.
    expires_in: u64,
}

/// Builds the exact form body the exchange sends.
fn build_form_body(
    credentials: &CloudServiceCredentials,
    assertion: &ServiceCredentialAssertion,
) -> Result<Vec<u8>, ExchangeFailure> {
    let contract = ProfileAuthenticationContract::embedded();
    let names = &contract.literals.identity_management_request_fields;
    let assertion_bytes = assertion.lend_compact_bytes(<[u8]>::to_vec);
    let values = [
        credentials.technical_account_client_identifier().as_bytes().to_vec(),
        credentials.client_secret().expose_secret_bytes().to_vec(),
        assertion_bytes,
    ];
    let mut body = Vec::new();
    for (position, name) in [CLIENT_FIELD, SECRET_FIELD, ASSERTION_FIELD].into_iter().zip(names) {
        if !body.is_empty() {
            body.push(b'&');
        }
        body.extend_from_slice(name.as_bytes());
        body.push(b'=');
        body.extend_from_slice(form_encode(&values[position]).as_bytes());
    }
    if u64::try_from(body.len()).unwrap_or(u64::MAX)
        > contract.limits.maximum_identity_management_request_body_bytes
    {
        return Err(ExchangeFailure::new(
            ConfigurationFailureCode::IdentityManagementResponseHeadLimitExceeded,
        ));
    }
    Ok(body)
}

/// Encodes one field value exactly as the contract spells it.
///
/// A space becomes a plus; the alphanumerics and the four literal punctuation
/// bytes stay as they are; every other byte becomes an uppercase percent
/// escape. The rule is written out because the reachable maximum body size is
/// derived from it, and a more generous encoder would make that maximum wrong.
fn form_encode(value: &[u8]) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value {
        match byte {
            b' ' => encoded.push('+'),
            _ if byte.is_ascii_alphanumeric() || b"*-._".contains(byte) => {
                encoded.push(char::from(*byte));
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

/// Charges and validates one response, in the contract's precedence order.
fn accept_response(response: &DecodedResponse) -> Result<TokenDocument, ExchangeFailure> {
    let contract = ProfileAuthenticationContract::embedded();
    let limits = &contract.limits;
    // Only the first informational head is charged and reported: the contract
    // closes the exchange there rather than reading whatever follows it.
    if let Some(head) = response.informational.first() {
        charge_head(&head.fields, limits.identity_management_response_head_status_charge_bytes)?;
        return Err(ExchangeFailure::new(
            ConfigurationFailureCode::IdentityManagementResponseStatusRejected,
        ));
    }
    charge_head(
        &response.head.fields,
        limits.identity_management_response_head_status_charge_bytes,
    )?;
    if u64::try_from(response.body.len()).unwrap_or(u64::MAX)
        > limits.maximum_identity_management_response_body_bytes
    {
        return Err(ExchangeFailure::new(
            ConfigurationFailureCode::IdentityManagementResponseBodyLimitExceeded,
        ));
    }
    let redirection = LOWEST_REDIRECTION..LOWEST_REDIRECTION + STATUS_CLASS_SPAN;
    if redirection.contains(&response.head.status) {
        return Err(ExchangeFailure::new(
            ConfigurationFailureCode::IdentityManagementRedirectRefused,
        ));
    }
    if u64::from(response.head.status) != limits.identity_management_response_success_status {
        return Err(ExchangeFailure::new(
            ConfigurationFailureCode::IdentityManagementResponseStatusRejected,
        ));
    }
    refuse_trailers(response, limits.identity_management_response_trailer_charge_bytes)?;
    accept_media(&response.head.fields)?;
    accept_document(&response.body)
}

/// Charges one decoded section against every bound the contract names.
fn charge_head(fields: &[(String, String)], base: u64) -> Result<(), ExchangeFailure> {
    let contract = ProfileAuthenticationContract::embedded();
    let limits = &contract.limits;
    let exceeded = || {
        ExchangeFailure::new(ConfigurationFailureCode::IdentityManagementResponseHeadLimitExceeded)
    };
    if u64::try_from(fields.len()).unwrap_or(u64::MAX)
        > limits.maximum_identity_management_response_header_count
    {
        return Err(exceeded());
    }
    let mut charged = base;
    for (name, value) in fields {
        let field = u64::try_from(name.len().checked_add(value.len()).ok_or_else(exceeded)?)
            .map_err(|_| exceeded())?;
        if field > limits.maximum_identity_management_response_header_bytes {
            return Err(exceeded());
        }
        charged = charged
            .checked_add(field)
            .and_then(|total| {
                total.checked_add(limits.identity_management_response_field_charge_bytes)
            })
            .ok_or_else(exceeded)?;
        if charged > limits.maximum_identity_management_response_head_bytes {
            return Err(exceeded());
        }
    }
    Ok(())
}

/// Refuses a declared or present trailer section.
///
/// A trailer arrives after the body, so a response that declares one is refused
/// before its body is read at all rather than after the exchange has already
/// committed to interpreting what follows.
fn refuse_trailers(response: &DecodedResponse, base: u64) -> Result<(), ExchangeFailure> {
    let rejected = || {
        ExchangeFailure::new(ConfigurationFailureCode::IdentityManagementResponseTrailerRejected)
    };
    if response.head.fields.iter().any(|(name, _)| name.eq_ignore_ascii_case(TRAILER_FIELD)) {
        return Err(rejected());
    }
    let Some(trailer) = &response.trailer else {
        return Ok(());
    };
    charge_head(&trailer.fields, base)?;
    Err(rejected())
}

/// Accepts only the media type and coding the contract names.
fn accept_media(fields: &[(String, String)]) -> Result<(), ExchangeFailure> {
    let literals = &ProfileAuthenticationContract::embedded().literals;
    let invalid =
        || ExchangeFailure::new(ConfigurationFailureCode::IdentityManagementResponseMediaInvalid);
    let codings: Vec<&String> = named(fields, CONTENT_ENCODING_FIELD);
    match codings.as_slice() {
        [] => {}
        [coding]
            if literals
                .accepted_content_codings
                .iter()
                .any(|known| coding.eq_ignore_ascii_case(known)) => {}
        _ => return Err(invalid()),
    }
    let media: Vec<&String> = named(fields, CONTENT_TYPE_FIELD);
    let [declared] = media.as_slice() else {
        return Err(invalid());
    };
    let mut parts = declared.split(';').map(str::trim);
    let media_type = parts.next().unwrap_or_default();
    if !media_type.eq_ignore_ascii_case(&literals.response_media_type) {
        return Err(invalid());
    }
    match parts.next() {
        None => Ok(()),
        Some(parameter)
            if parameter.eq_ignore_ascii_case(&literals.response_charset_parameter)
                && parts.next().is_none() =>
        {
            Ok(())
        }
        Some(_) => Err(invalid()),
    }
}

/// Returns every value one field name carries.
fn named<'fields>(fields: &'fields [(String, String)], wanted: &str) -> Vec<&'fields String> {
    fields
        .iter()
        .filter(|(name, _)| name.eq_ignore_ascii_case(wanted))
        .map(|(_, value)| value)
        .collect()
}

/// Accepts only the closed response document the contract names.
fn accept_document(body: &[u8]) -> Result<TokenDocument, ExchangeFailure> {
    let contract = ProfileAuthenticationContract::embedded();
    let limits = &contract.limits;
    let invalid = || {
        ExchangeFailure::new(ConfigurationFailureCode::IdentityManagementResponseDocumentInvalid)
    };
    let document: ResponseDocument = serde_json::from_slice(body).map_err(|_| invalid())?;
    if !document.token_type.eq_ignore_ascii_case(&contract.literals.response_token_type)
        || document.token_type != contract.literals.response_token_type
    {
        return Err(ExchangeFailure::new(
            ConfigurationFailureCode::IdentityManagementTokenTypeInvalid,
        ));
    }
    let length = u64::try_from(document.access_token.len()).unwrap_or(u64::MAX);
    if document.access_token.is_empty()
        || length > limits.maximum_access_token_bytes
        || !document.access_token.bytes().all(is_token_byte)
    {
        return Err(invalid());
    }
    if document.expires_in == 0
        || document.expires_in > limits.maximum_access_token_lifetime_milliseconds
    {
        return Err(ExchangeFailure::new(
            ConfigurationFailureCode::IdentityManagementTokenLifetimeInvalid,
        ));
    }
    Ok(TokenDocument { access_token: document.access_token, expires_in: document.expires_in })
}

/// Reports whether one byte may appear in an access token.
fn is_token_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"-._~+/=".contains(&byte)
}

/// The response document exactly as it is spelled.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ResponseDocument {
    /// The token itself.
    access_token: String,
    /// The type the response declares.
    token_type: String,
    /// The lifetime the response advertises, in milliseconds.
    expires_in: u64,
}

/// Installs one accepted document as a token, or refuses its lease.
///
/// The deadline is measured from the anchor taken before the first request
/// byte, never from the moment the body arrived, so writing, waiting, and
/// reading all consume the lifetime the endpoint advertised rather than
/// extending it.
fn install(
    document: TokenDocument,
    anchor: u64,
    receipt: u64,
) -> Result<AccessToken, ExchangeFailure> {
    let limits = &ProfileAuthenticationContract::embedded().limits;
    let too_short = || ExchangeFailure::new(ConfigurationFailureCode::AccessTokenLifetimeTooShort);
    let deadline = anchor.checked_add(document.expires_in).ok_or_else(too_short)?;
    let usable = deadline.checked_sub(receipt).ok_or_else(too_short)?;
    let required = limits
        .access_token_refresh_skew_milliseconds
        .checked_add(limits.minimum_access_token_usable_lease_milliseconds)
        .ok_or_else(too_short)?;
    if usable <= required {
        return Err(too_short());
    }
    Ok(AccessToken {
        token: SecretValue::from_text(document.access_token),
        deadline_milliseconds: deadline,
    })
}

/// Returns the complete endpoint the exchange constructs for itself.
///
/// It is built from the manifest alone. No part of a credential reaches it, so
/// a credential naming another host cannot move the secret it carries.
#[must_use]
pub fn identity_management_endpoint() -> String {
    let literals = &ProfileAuthenticationContract::embedded().literals;
    format!(
        "{}://{}{}",
        literals.identity_management_scheme,
        literals.identity_management_authorities[0],
        literals.identity_management_endpoint_path
    )
}
