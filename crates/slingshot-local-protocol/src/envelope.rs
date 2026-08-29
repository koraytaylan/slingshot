//! Stable request and response envelopes of the retained control surface.
//!
//! Every retained control message carries the control version, the caller's
//! request identifier, and either a method with its arguments or an outcome
//! with its result or structured error. The serialized field names are stable
//! and spelled in full, and every bound comes from the foundation contract.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::foundation_contract::FoundationContract;
use crate::framing::{self, FramingFailure};

/// Error code returned when a payload is not a readable control message.
pub const MALFORMED_REQUEST_CODE: &str = "malformed_request";

/// Error code returned when a payload breaks a declared bound.
pub const LIMIT_EXCEEDED_CODE: &str = "limit_exceeded";

/// Error code returned when the control version is not the retained one.
pub const UNSUPPORTED_CONTROL_VERSION_CODE: &str = "retained_control_version_unsupported";

/// Error code returned when the method is outside the retained surface.
pub const METHOD_NOT_FOUND_CODE: &str = "method_not_found";

/// Error code returned when a stop names a nonce a prior instance published.
pub const STALE_DAEMON_INSTANCE_CODE: &str = "stale_daemon_instance";

/// Outcome of one control request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResponseOutcome {
    /// The request was served and carries a result.
    Success,
    /// The request was refused and carries a structured error.
    Failure,
}

/// One structured refusal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlError {
    /// Stable code a caller may branch on.
    pub code: String,
    /// Bounded description of the refusal.
    pub message: String,
}

impl ControlError {
    /// Builds one structured refusal, truncating nothing.
    #[must_use]
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self { code: code.to_owned(), message: message.into() }
    }
}

/// One retained control request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlRequest {
    /// Retained control version the caller speaks.
    pub control_version: u32,
    /// Identifier the caller correlates its response with.
    pub request_identifier: String,
    /// Retained method the caller is invoking.
    pub method: String,
    /// Arguments of the method.
    pub arguments: Value,
}

/// One retained control response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlResponse {
    /// Retained control version the daemon speaks.
    pub control_version: u32,
    /// Identifier the caller correlates this response with.
    pub request_identifier: String,
    /// Whether the request was served or refused.
    pub outcome: ResponseOutcome,
    /// Result of a served request.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    /// Structured refusal of a request that was not served.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ControlError>,
}

/// A request the daemon refused before dispatching a method.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefusedRequest {
    /// Identifier the refusal correlates with, when the payload carried one.
    pub request_identifier: Option<String>,
    /// Structured refusal.
    pub error: ControlError,
}

/// Reason a response could not be rendered.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RenderFailure {
    /// The result could not be rendered as a value.
    #[error("the result could not be rendered: {0}")]
    UnrenderableResult(String),
    /// The rendered response is beyond the contract's frame limit.
    #[error("the rendered response is beyond the frame limit")]
    BeyondFrameLimit,
}

/// Only the fields a refusal needs before the message is fully understood.
#[derive(Debug, Deserialize)]
struct CorrelationOnly {
    /// Identifier the caller supplied, when it supplied one.
    request_identifier: Option<String>,
}

/// Translates a framing failure into the structured error it reports.
fn framing_error(failure: &FramingFailure) -> ControlError {
    let code = match failure {
        FramingFailure::PayloadTooLarge { .. }
        | FramingFailure::NestingTooDeep { .. }
        | FramingFailure::CollectionTooLarge { .. } => LIMIT_EXCEEDED_CODE,
        _ => MALFORMED_REQUEST_CODE,
    };
    ControlError::new(code, failure.to_string())
}

/// Reads the caller's identifier out of a payload that failed later checks.
fn correlation_of(payload: &[u8]) -> Option<String> {
    serde_json::from_slice::<CorrelationOnly>(payload)
        .ok()
        .and_then(|found| found.request_identifier)
}

/// Reports every name bound one request breaks.
fn evaluate_name_bounds(
    contract: &FoundationContract,
    request: &ControlRequest,
) -> Option<ControlError> {
    let bounds = [
        (
            "request_identifier",
            request.request_identifier.len(),
            contract.names.request_identifier_bytes,
        ),
        ("method", request.method.len(), contract.names.method_bytes),
    ];
    bounds.into_iter().find(|(_, length, limit)| *length > *limit as usize).map(
        |(name, length, limit)| {
            ControlError::new(
                LIMIT_EXCEEDED_CODE,
                format!("{name} is {length} bytes, beyond the limit of {limit}"),
            )
        },
    )
}

/// Reads one control request out of a frame payload.
///
/// The control version is compared before any method is dispatched, so a caller
/// speaking another version receives the compatibility refusal rather than a
/// method result or a method-not-found refusal.
///
/// # Errors
///
/// Returns [`RefusedRequest`] carrying the structured error and, whenever the
/// payload supplied one, the caller's request identifier.
pub fn decode_request(
    contract: &FoundationContract,
    payload: &[u8],
) -> Result<ControlRequest, RefusedRequest> {
    let refuse =
        |error: ControlError| RefusedRequest { request_identifier: correlation_of(payload), error };
    let text = framing::read_payload(&contract.framing, payload)
        .map_err(|failure| refuse(framing_error(&failure)))?;
    let request: ControlRequest = serde_json::from_str(text).map_err(|failure| {
        refuse(ControlError::new(MALFORMED_REQUEST_CODE, failure.to_string()))
    })?;
    if let Some(error) = evaluate_name_bounds(contract, &request) {
        return Err(RefusedRequest { request_identifier: Some(request.request_identifier), error });
    }
    if request.control_version != contract.control.version {
        let message = format!(
            "the retained control version is {}, not {}",
            contract.control.version, request.control_version
        );
        return Err(RefusedRequest {
            request_identifier: Some(request.request_identifier),
            error: ControlError::new(UNSUPPORTED_CONTROL_VERSION_CODE, message),
        });
    }
    Ok(request)
}

impl ControlResponse {
    /// Builds a response carrying the result of a served request.
    ///
    /// # Errors
    ///
    /// Returns [`RenderFailure::UnrenderableResult`] when the result cannot be
    /// rendered as a value.
    pub fn served(
        contract: &FoundationContract,
        request_identifier: &str,
        result: &impl Serialize,
    ) -> Result<Self, RenderFailure> {
        let rendered = serde_json::to_value(result)
            .map_err(|failure| RenderFailure::UnrenderableResult(failure.to_string()))?;
        Ok(Self {
            control_version: contract.control.version,
            request_identifier: request_identifier.to_owned(),
            outcome: ResponseOutcome::Success,
            result: Some(rendered),
            error: None,
        })
    }

    /// Builds a response carrying a structured refusal.
    #[must_use]
    pub fn refused(
        contract: &FoundationContract,
        request_identifier: &str,
        error: ControlError,
    ) -> Self {
        Self {
            control_version: contract.control.version,
            request_identifier: request_identifier.to_owned(),
            outcome: ResponseOutcome::Failure,
            result: None,
            error: Some(error),
        }
    }

    /// Renders this response as one frame.
    ///
    /// # Errors
    ///
    /// Returns [`RenderFailure`] when the response cannot be rendered or is
    /// beyond the contract's frame limit.
    pub fn render_frame(&self, contract: &FoundationContract) -> Result<Vec<u8>, RenderFailure> {
        let payload = serde_json::to_vec(self)
            .map_err(|failure| RenderFailure::UnrenderableResult(failure.to_string()))?;
        framing::render(&contract.framing, &payload).map_err(|_| RenderFailure::BeyondFrameLimit)
    }
}
