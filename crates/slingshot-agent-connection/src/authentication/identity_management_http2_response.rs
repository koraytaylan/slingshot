//! Bounded IMS HTTP/2 assembly. A validated END_STREAM completes the response;
//! transport EOF is not required when the peer keeps the connection open.

use super::{
    identity_management_exchange::{DecodedHead, DecodedResponse, ExchangeFailure, accept_media},
    identity_management_http2_headers::IdentityManagementHeadReader,
};
use crate::{
    selected_author_hpack_block::ResponseBlock,
    selected_author_http2_flow::ReceiveWindows,
    selected_author_http2_frames::{ResponseFrame, TransportEnd},
};
use slingshot_domain::{
    profile_authentication_contract::{
        ConfigurationFailureCode as Code, ProfileAuthenticationContract,
    },
    secret_value::SecretValue,
};

/// HTTP/2 WINDOW_UPDATE: a nine-byte frame head and four-byte increment.
const WINDOW_UPDATE_FRAME_BYTES: usize = 13;
/// A received payload restores both connection and request-stream credit.
const RECEIVE_WINDOW_COUNT: usize = 2;
/// HTTP/2 CONTINUATION frame type.
const CONTINUATION_FRAME: u8 = 9;
/// HTTP/2 END_HEADERS flag.
const END_HEADERS: u8 = 4;
/// Inclusive first redirect status.
const REDIRECT_STATUS_START: u16 = 300;
/// Exclusive end of redirect statuses.
const REDIRECT_STATUS_END: u16 = 400;

/// One ordered response on stream 1. Wire validation and control-frame handling
/// stay with the enclosing driver; every response frame must reach this object.
pub struct IdentityManagementHttp2Response {
    block: Option<ResponseBlock<IdentityManagementHeadReader>>,
    in_block: bool,
    header_end_stream: bool,
    head: Option<DecodedHead>,
    ended: bool,
    expected_length: Option<u64>,
    body: Vec<u8>,
    windows: ReceiveWindows,
    failure: Option<ExchangeFailure>,
}
impl ::core::fmt::Debug for IdentityManagementHttp2Response {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("IdentityManagementHttp2Response([redacted])")
    }
}
impl Drop for IdentityManagementHttp2Response {
    fn drop(&mut self) {
        let _secret = SecretValue::from_bytes(std::mem::take(&mut self.body));
    }
}
impl Default for IdentityManagementHttp2Response {
    fn default() -> Self {
        Self::new()
    }
}
impl IdentityManagementHttp2Response {
    /// Uses only the IMS manifest bounds and a fresh connection compression table.
    pub fn new() -> Self {
        Self {
            block: Some(ResponseBlock::with_reader(
                IdentityManagementHeadReader::response(),
                maximum_head(),
            )),
            in_block: false,
            header_end_stream: false,
            head: None,
            ended: false,
            expected_length: None,
            body: Vec::with_capacity(maximum_body() as usize),
            windows: ReceiveWindows::new(),
            failure: None,
        }
    }
    /// Whether the initial head passed complete accounting/status/media checks.
    pub fn head_complete(&self) -> bool {
        self.head.is_some() && self.failure.is_none()
    }
    /// Whether a complete framed stream ended without a partial head or refusal.
    pub fn stream_ended(&self) -> bool {
        self.ended && !self.in_block && self.failure.is_none()
    }
    /// Returns the first stable refusal without peer-controlled text.
    pub fn failure(&self) -> Option<ExchangeFailure> {
        self.failure
    }
    /// Admits one wire-validated response frame. Credits are returned only after
    /// bounded body admission; cancellation never restores unconsumed credit.
    ///
    /// # Errors
    /// Rejects invalid frame order, headers, status, media, body bounds, or
    /// declared lengths. Once refused, every later frame returns that refusal.
    pub fn accept(
        &mut self,
        frame: &ResponseFrame,
    ) -> Result<Option<[[u8; WINDOW_UPDATE_FRAME_BYTES]; RECEIVE_WINDOW_COUNT]>, ExchangeFailure>
    {
        if let Some(failure) = self.failure {
            return Err(failure);
        }
        let result = self.accept_inner(frame);
        if let Err(failure) = result {
            self.failure = Some(failure);
        }
        result
    }
    fn accept_inner(
        &mut self,
        frame: &ResponseFrame,
    ) -> Result<Option<[[u8; WINDOW_UPDATE_FRAME_BYTES]; RECEIVE_WINDOW_COUNT]>, ExchangeFailure>
    {
        if frame.stream_identifier != 1 || self.ended {
            return Err(malformed());
        }
        match frame.kind {
            1 | CONTINUATION_FRAME => self.accept_headers(frame).map(|()| None),
            0 => self.accept_data(frame),
            _ => Err(malformed()),
        }
    }

    fn accept_headers(&mut self, frame: &ResponseFrame) -> Result<(), ExchangeFailure> {
        if frame.kind == 1 {
            if self.in_block || self.head.is_some() && frame.flags & 1 == 0 {
                return Err(malformed());
            }
            self.in_block = true;
            self.header_end_stream = frame.flags & 1 != 0;
        } else if !self.in_block {
            return Err(malformed());
        }
        let block = self.block.as_mut().ok_or_else(malformed)?;
        if block.push(&frame.payload).is_err() {
            return Err(ExchangeFailure::new(if block.encoded_limit_exceeded() {
                Code::IdentityManagementResponseHeadLimitExceeded
            } else {
                block.reader().failure_code().unwrap_or(Code::IdentityManagementTransportFailed)
            }));
        }
        if frame.flags & END_HEADERS != 0 {
            self.finish_headers()?;
        }
        Ok(())
    }

    fn finish_headers(&mut self) -> Result<(), ExchangeFailure> {
        let block = self.block.take().ok_or_else(malformed)?;
        if self.head.is_some() {
            block.finish().map_err(|_| malformed())?;
            return Err(ExchangeFailure::new(Code::IdentityManagementResponseTrailerRejected));
        }
        let (section, next) = block
            .finish_with_next_reader(IdentityManagementHeadReader::trailers(), maximum_head())
            .map_err(|_| malformed())?;
        self.block = Some(next);
        self.in_block = false;
        self.install_head(DecodedHead {
            status: section.status.ok_or_else(malformed)?,
            fields: section.fields,
        })
    }

    fn accept_data(
        &mut self,
        frame: &ResponseFrame,
    ) -> Result<Option<[[u8; WINDOW_UPDATE_FRAME_BYTES]; RECEIVE_WINDOW_COUNT]>, ExchangeFailure>
    {
        if self.head.is_none() || self.in_block {
            return Err(malformed());
        }
        let (permit, content) = self.windows.receive_frame(frame).map_err(|_| malformed())?;
        let length =
            (self.body.len() as u64).checked_add(content.len() as u64).ok_or_else(|| {
                ExchangeFailure::new(Code::IdentityManagementResponseBodyLimitExceeded)
            })?;
        if length > maximum_body() {
            return Err(ExchangeFailure::new(Code::IdentityManagementResponseBodyLimitExceeded));
        }
        if self.expected_length.is_some_and(|expected| length > expected) {
            return Err(malformed());
        }
        self.body.extend_from_slice(content);
        self.ended = frame.flags & 1 != 0;
        if self.ended && self.expected_length.is_some_and(|expected| expected != length) {
            return Err(malformed());
        }
        Ok(permit.release())
    }
    fn install_head(&mut self, head: DecodedHead) -> Result<(), ExchangeFailure> {
        let length = declared_length(&head.fields)?;
        if (REDIRECT_STATUS_START..REDIRECT_STATUS_END).contains(&head.status) {
            return Err(ExchangeFailure::new(Code::IdentityManagementRedirectRefused));
        }
        if u64::from(head.status)
            != ProfileAuthenticationContract::embedded()
                .limits
                .identity_management_response_success_status
        {
            return Err(ExchangeFailure::new(Code::IdentityManagementResponseStatusRejected));
        }
        if head.fields.iter().any(|(name, _)| name == "trailer") {
            return Err(ExchangeFailure::new(Code::IdentityManagementResponseTrailerRejected));
        }
        accept_media(&head.fields)?;
        if length.is_some_and(|length| length > maximum_body()) {
            return Err(ExchangeFailure::new(Code::IdentityManagementResponseBodyLimitExceeded));
        }
        if self.header_end_stream && length.is_some_and(|length| length != 0) {
            return Err(malformed());
        }
        self.expected_length = length;
        self.ended = self.header_end_stream;
        self.head = Some(head);
        Ok(())
    }
    /// Consumes the frame reader's final EOF proof. Partial or poisoned assembly
    /// never exposes its head/body; token validation remains the source's job.
    ///
    /// # Errors
    /// Returns the retained refusal or rejects an incomplete stream or absent head.
    pub fn finish_at_transport_end(
        mut self,
        _end: TransportEnd,
    ) -> Result<DecodedResponse, ExchangeFailure> {
        if let Some(failure) = self.failure {
            return Err(failure);
        }
        if !self.stream_ended() {
            return Err(malformed());
        }
        Ok(DecodedResponse {
            informational: Vec::new(),
            head: self.head.take().ok_or_else(malformed)?,
            body: std::mem::take(&mut self.body),
            trailer: None,
        })
    }

    /// Consumes a complete HTTP/2 END_STREAM proof. HTTP/2 streams may end
    /// while the TLS connection remains reusable; transport EOF is not part
    /// of the response boundary.
    ///
    /// # Errors
    /// Returns the retained refusal or rejects an incomplete stream or absent head.
    pub fn finish_at_stream_end(mut self) -> Result<DecodedResponse, ExchangeFailure> {
        if let Some(failure) = self.failure {
            return Err(failure);
        }
        if !self.stream_ended() {
            return Err(malformed());
        }
        Ok(DecodedResponse {
            informational: Vec::new(),
            head: self.head.take().ok_or_else(malformed)?,
            body: std::mem::take(&mut self.body),
            trailer: None,
        })
    }
}
fn declared_length(fields: &[(String, String)]) -> Result<Option<u64>, ExchangeFailure> {
    let mut length = None;
    for (_, value) in fields.iter().filter(|(name, _)| name == "content-length") {
        if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(malformed());
        }
        let parsed = value.parse::<u64>().map_err(|_| malformed())?;
        if length.is_some_and(|length| length != parsed) {
            return Err(malformed());
        }
        length = Some(parsed);
    }
    Ok(length)
}
fn maximum_head() -> u64 {
    ProfileAuthenticationContract::embedded().limits.maximum_identity_management_response_head_bytes
}
fn maximum_body() -> u64 {
    ProfileAuthenticationContract::embedded().limits.maximum_identity_management_response_body_bytes
}
fn malformed() -> ExchangeFailure {
    ExchangeFailure::new(Code::IdentityManagementTransportFailed)
}

#[cfg(test)]
#[path = "identity_management_http2_response_tests.rs"]
mod tests;
