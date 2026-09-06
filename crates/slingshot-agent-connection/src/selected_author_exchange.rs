//! Strict finite-response decoding for the selected author connection.
//!
//! The network driver supplies a collected finite response only after its
//! phase deadlines have elapsed successfully.  This module is the last common
//! gate before a route-specific codec sees status or body bytes: it accounts
//! for every decoded header, rejects a redirect or content coding, and applies
//! the transport contract's finite-body bound.

use http::{HeaderMap, Response, Version};
use slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract;

use crate::author_hypertext_transfer_protocol_policy::{
    HeadBounds, HeadReader, ResponseHead, ResponseRefusal,
};

/// A finite author response that passed the common wire policy.
#[derive(Clone, PartialEq, Eq)]
pub struct SelectedAuthorFiniteResponse {
    /// The validated status code.
    pub status: u16,
    /// The validated, uncompressed body.
    pub body: Vec<u8>,
    /// The policy-validated final response head.
    ///
    /// Route codecs retain this rather than rebuilding it from headers, so a
    /// response cannot acquire different framing semantics between the common
    /// gate and a route-specific decision.
    pub head: ResponseHead,
    /// The one content type the response declared, when it declared one.
    ///
    /// Its exact spelling is preserved for the route codec to validate.
    pub content_type: Option<String>,
    /// The one retry directive the response declared, when it declared one.
    ///
    /// The common gate does not interpret its value because each route owns
    /// the applicable retry grammar, but it refuses ambiguous repetition.
    pub retry_after: Option<String>,
}

impl core::fmt::Debug for SelectedAuthorFiniteResponse {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("SelectedAuthorFiniteResponse([redacted])")
    }
}

/// The wire observations a bounded transport must make before exposing a
/// finite author response.
///
/// `http::Response` deliberately represents only the final head and body. It
/// cannot say whether a transport saw an actual trailer block or bytes after
/// the message boundary, so those facts cannot be safely inferred by a route
/// codec from the response alone.
pub struct CollectedFiniteResponse {
    /// The final response head and the exact identity-coded body bytes.
    pub response: Response<Vec<u8>>,
    /// Whether message framing had more than one valid interpretation.
    pub framing_ambiguous: bool,
    /// Whether an actual HTTP trailer section arrived.
    pub trailer_section_present: bool,
    /// Whether bytes followed the proved message boundary.
    pub trailing_bytes: bool,
}

impl core::fmt::Debug for CollectedFiniteResponse {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("CollectedFiniteResponse([redacted])")
    }
}

/// Why a finite response cannot reach a route-specific codec.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SelectedAuthorExchangeRefusal {
    /// Only a nonredirect final status can reach route interpretation.
    #[error("the author response status is not an acceptable final status")]
    Status,
    /// Every protocol response must declare its media type.
    #[error("the author response has no content type")]
    MissingContentType,
    /// The shared response policy refused its head.
    #[error(transparent)]
    Head(#[from] ResponseRefusal),
    /// A header was not valid text and therefore could not be interpreted.
    #[error("an author response header is not valid text")]
    HeaderNotText,
    /// A singleton policy header appeared more than once.
    #[error("the {name} response header appeared more than once")]
    DuplicateSingletonHeader {
        /// The header whose meaning would otherwise be ambiguous.
        name: &'static str,
    },
    /// The transport could not prove one interpretation of the message body.
    #[error("the author response framing is ambiguous")]
    FramingAmbiguous,
    /// A trailer section arrived after the finite body.
    #[error("the author response carried a trailer section")]
    TrailerSectionPresent,
    /// The socket carried bytes after the complete response message.
    #[error("the author response carried bytes after its message boundary")]
    TrailingBytes,
    /// The finite body exceeded the contract maximum before a codec read it.
    #[error("an author finite response holds at most {allowed} bytes, and this holds {actual}")]
    BodyTooLong {
        /// The transport contract maximum.
        allowed: u64,
        /// The received body length.
        actual: usize,
    },
}

/// Validates one complete finite response from the selected author.
///
/// The transport must not follow redirects or decompress before calling this
/// function. The head's singleton fields are rejected when repeated, rather
/// than allowing a library-specific first/last interpretation.
pub fn validate_collected_finite_response(
    collected: CollectedFiniteResponse,
) -> Result<SelectedAuthorFiniteResponse, SelectedAuthorExchangeRefusal> {
    if collected.framing_ambiguous {
        return Err(SelectedAuthorExchangeRefusal::FramingAmbiguous);
    }
    if collected.trailer_section_present {
        return Err(SelectedAuthorExchangeRefusal::TrailerSectionPresent);
    }
    if collected.trailing_bytes {
        return Err(SelectedAuthorExchangeRefusal::TrailingBytes);
    }
    let (parts, body) = collected.response.into_parts();
    let (head, content_type) = validate_finite_head(parts.status, parts.version, &parts.headers)?;
    let limit =
        AuthorAgentTransportContract::embedded().limit("maximum_finite_response_body_bytes");
    if u64::try_from(body.len()).unwrap_or(u64::MAX) > limit {
        return Err(SelectedAuthorExchangeRefusal::BodyTooLong {
            allowed: limit,
            actual: body.len(),
        });
    }
    Ok(SelectedAuthorFiniteResponse {
        status: parts.status.as_u16(),
        body,
        content_type: Some(content_type),
        retry_after: singleton(&parts.headers, "retry-after")?,
        head,
    })
}

/// The same finite head policy before collecting a streaming transport's body.
pub(crate) fn validate_finite_head(
    status: http::StatusCode,
    version: Version,
    headers: &HeaderMap,
) -> Result<(ResponseHead, String), SelectedAuthorExchangeRefusal> {
    if status.as_u16() < 200 || status.is_redirection() || status.as_u16() >= 600 {
        return Err(SelectedAuthorExchangeRefusal::Status);
    }
    let head = response_head(version, headers)?;
    head.require_acceptable()?;
    let content_type = singleton(headers, "content-type")?
        .ok_or(SelectedAuthorExchangeRefusal::MissingContentType)?;
    singleton(headers, "retry-after")?;
    Ok((head, content_type))
}

/// Builds the policy head while charging every decoded header exactly once.
fn response_head(
    version: Version,
    headers: &HeaderMap,
) -> Result<ResponseHead, SelectedAuthorExchangeRefusal> {
    let mut reader = HeadReader::new(HeadBounds::embedded());
    for (name, value) in headers {
        reader.read_field(
            name.as_str(),
            value.to_str().map_err(|_| SelectedAuthorExchangeRefusal::HeaderNotText)?,
        )?;
    }
    Ok(ResponseHead {
        content_coding: singleton(headers, "content-encoding")?,
        location: singleton(headers, "location")?,
        protocol_version: match version {
            Version::HTTP_11 => "HTTP/1.1".to_owned(),
            Version::HTTP_2 => "HTTP/2".to_owned(),
            other => format!("{other:?}"),
        },
        trailers_declared: headers.contains_key("trailer"),
        alternative_service_offered: headers.contains_key("alt-svc"),
        informational: false,
    })
}

/// Returns one singleton header or refuses its ambiguous repetition.
fn singleton(
    headers: &HeaderMap,
    name: &'static str,
) -> Result<Option<String>, SelectedAuthorExchangeRefusal> {
    let values: Vec<_> = headers.get_all(name).iter().collect();
    match values.as_slice() {
        [] => Ok(None),
        [value] => Ok(Some(
            value.to_str().map_err(|_| SelectedAuthorExchangeRefusal::HeaderNotText)?.to_owned(),
        )),
        _ => Err(SelectedAuthorExchangeRefusal::DuplicateSingletonHeader { name }),
    }
}
