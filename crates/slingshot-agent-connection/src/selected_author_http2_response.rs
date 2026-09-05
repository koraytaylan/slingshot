//! Finite HTTP/2 response assembly behind bounded framing and HPACK gates.
//! No partial response is exposed. Connection control frames and the final
//! transport boundary remain the enclosing network driver's responsibility.

use crate::selected_author_exchange::{
    CollectedFiniteResponse, SelectedAuthorFiniteResponse, validate_collected_finite_response,
    validate_finite_head,
};
use crate::selected_author_hpack_block::ResponseBlock;
use crate::selected_author_http2_flow::ReceiveWindows;
use crate::selected_author_http2_frames::ResponseFrame;
use http::{HeaderMap, Response, StatusCode, Version};
use slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract;

/// Invalid or incomplete finite response; no private head/body data is retained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the author HTTP/2 finite response is invalid")]
pub struct ResponseRefusal;

/// One bounded finite response on stream 1. The network driver must pass every
/// response HEADERS/CONTINUATION/DATA frame, not discard inconvenient frames.
pub struct FiniteResponse {
    block: Option<ResponseBlock>,
    head: Option<(StatusCode, HeaderMap)>,
    header_end_stream: bool,
    ended: bool,
    expected_length: Option<u64>,
    maximum: u64,
    body: Vec<u8>,
    windows: ReceiveWindows,
    poisoned: bool,
}

impl core::fmt::Debug for FiniteResponse {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("FiniteResponse([redacted])")
    }
}

impl Default for FiniteResponse {
    fn default() -> Self {
        Self::new()
    }
}

impl FiniteResponse {
    /// Uses the installed finite-body limit, separate from encoded/decoded heads.
    pub fn new() -> Self {
        Self {
            block: None,
            head: None,
            header_end_stream: false,
            ended: false,
            expected_length: None,
            maximum: AuthorAgentTransportContract::embedded()
                .limit("maximum_finite_response_body_bytes"),
            body: Vec::new(),
            windows: ReceiveWindows::new(),
            poisoned: false,
        }
    }

    /// Whether END_STREAM completed the response, including its full head.
    /// This is not proof of a trailer-free final transport boundary.
    pub fn stream_ended(&self) -> bool {
        self.ended && !self.poisoned
    }

    /// Whether the complete bounded head passed the shared finite-head policy.
    pub fn head_complete(&self) -> bool {
        self.head.is_some() && !self.poisoned
    }

    /// Processes a wire-validated response frame and returns receive-credit
    /// frames only after bounded body storage succeeds. The driver must send
    /// those credits before receiving more DATA, or discard the connection.
    pub fn accept(
        &mut self,
        frame: &ResponseFrame,
    ) -> Result<Option<[[u8; 13]; 2]>, ResponseRefusal> {
        if self.poisoned || self.ended || frame.stream_identifier != 1 {
            self.poisoned = true;
            return Err(ResponseRefusal);
        }
        self.poisoned = true;
        let credits = match frame.kind {
            1 | 9 => {
                if self.head.is_some() {
                    return Err(ResponseRefusal);
                }
                if frame.kind == 1 {
                    if self.block.is_some() {
                        return Err(ResponseRefusal);
                    }
                    self.block = Some(ResponseBlock::new());
                    self.header_end_stream = frame.flags & 1 != 0;
                }
                let block = self.block.as_mut().ok_or(ResponseRefusal)?;
                block.push(&frame.payload).map_err(|_| ResponseRefusal)?;
                if frame.flags & 4 != 0 {
                    let (status, headers) = self
                        .block
                        .take()
                        .unwrap()
                        .finish()
                        .map_err(|_| ResponseRefusal)?
                        .into_parts();
                    self.install_head(status, headers, self.header_end_stream)?;
                }
                None
            }
            0 => {
                let (status, _) = self.head.as_ref().ok_or(ResponseRefusal)?;
                if self.block.is_some() {
                    return Err(ResponseRefusal);
                }
                let (permit, content) =
                    self.windows.receive_frame(frame).map_err(|_| ResponseRefusal)?;
                let length = (self.body.len() as u64)
                    .checked_add(content.len() as u64)
                    .filter(|length| *length <= self.maximum)
                    .ok_or(ResponseRefusal)?;
                if self.expected_length.is_some_and(|expected| length > expected)
                    || matches!(*status, StatusCode::NO_CONTENT | StatusCode::RESET_CONTENT)
                        && !content.is_empty()
                {
                    return Err(ResponseRefusal);
                }
                self.body.extend_from_slice(content);
                self.ended = frame.flags & 1 != 0;
                if self.ended && self.expected_length.is_some_and(|expected| length != expected) {
                    return Err(ResponseRefusal);
                }
                permit.release()
            }
            _ => return Err(ResponseRefusal),
        };
        self.poisoned = false;
        Ok(credits)
    }

    /// Adopts a head already passed through the bounded HPACK decoder, for a
    /// route which chooses finite error handling after inspecting its status.
    pub(crate) fn from_decoded_head(
        status: StatusCode,
        headers: HeaderMap,
        ended: bool,
    ) -> Result<Self, ResponseRefusal> {
        let mut response = Self::new();
        response.install_head(status, headers, ended)?;
        Ok(response)
    }

    fn install_head(
        &mut self,
        status: StatusCode,
        headers: HeaderMap,
        ended: bool,
    ) -> Result<(), ResponseRefusal> {
        validate_finite_head(status, Version::HTTP_2, &headers).map_err(|_| ResponseRefusal)?;
        self.expected_length = declared_length(&headers)?;
        if self.expected_length.is_some_and(|length| {
            length > self.maximum
                || status == StatusCode::NO_CONTENT
                || status == StatusCode::RESET_CONTENT && length != 0
                || ended && length != 0
        }) {
            return Err(ResponseRefusal);
        }
        self.head = Some((status, headers));
        self.ended = ended;
        Ok(())
    }

    /// Requires the frame reader's clean transport-end proof as well as a
    /// complete validated response. END_STREAM alone cannot finish a response.
    pub fn finish_at_transport_end(
        self,
        _end: crate::selected_author_http2_frames::TransportEnd,
    ) -> Result<SelectedAuthorFiniteResponse, ResponseRefusal> {
        if !self.stream_ended() || self.block.is_some() {
            return Err(ResponseRefusal);
        }
        let (status, headers) = self.head.ok_or(ResponseRefusal)?;
        let mut response = Response::new(self.body);
        *response.status_mut() = status;
        *response.version_mut() = Version::HTTP_2;
        *response.headers_mut() = headers;
        validate_collected_finite_response(CollectedFiniteResponse {
            response,
            framing_ambiguous: false,
            trailer_section_present: false,
            trailing_bytes: false,
        })
        .map_err(|_| ResponseRefusal)
    }
}

pub(crate) fn declared_length(headers: &HeaderMap) -> Result<Option<u64>, ResponseRefusal> {
    let mut lengths = headers.get_all("content-length").iter();
    let Some(value) = lengths.next() else {
        return Ok(None);
    };
    if lengths.next().is_some() {
        return Err(ResponseRefusal);
    }
    let value = value.to_str().map_err(|_| ResponseRefusal)?;
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(ResponseRefusal);
    }
    Ok(Some(value.parse().map_err(|_| ResponseRefusal)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn bounded_wire_frames_and_hpack_complete_one_finite_response() {
        use crate::selected_author_http2_frames::ResponseFrameReader;
        fn encode(frame: ResponseFrame, bytes: &mut Vec<u8>) {
            let length = (frame.payload.len() as u32).to_be_bytes();
            bytes.extend_from_slice(&[length[1], length[2], length[3], frame.kind, frame.flags]);
            bytes.extend_from_slice(&frame.stream_identifier.to_be_bytes());
            bytes.extend_from_slice(&frame.payload);
        }
        let mut wire = vec![0, 0, 0, 4, 0, 0, 0, 0, 0];
        let mut initial = head(&[("content-length", "2")], false);
        let tail = initial.payload.split_off(5);
        initial.flags = 0;
        encode(initial, &mut wire);
        encode(ResponseFrame { kind: 9, flags: 4, stream_identifier: 1, payload: tail }, &mut wire);
        encode(data(&[1, b'{', b'}', 0], 9), &mut wire);
        let mut input = wire.as_slice();
        let mut frames = ResponseFrameReader::new();
        assert_eq!(frames.read(&mut input).await.unwrap().kind, 4);
        let mut response = FiniteResponse::new();
        while !response.stream_ended() {
            let frame = frames.read(&mut input).await.unwrap();
            response.accept(&frame).unwrap();
        }
        let crate::selected_author_http2_frames::FrameRead::End(end) =
            frames.read_next(&mut input).await.unwrap()
        else {
            panic!("expected clean transport EOF");
        };
        let completed = response.finish_at_transport_end(end).unwrap();
        assert_eq!(completed.body, b"{}");
        assert_eq!(completed.content_type.as_deref(), Some("application/json"));
    }

    fn head(extra: &[(&str, &str)], ended: bool) -> ResponseFrame {
        let mut payload = vec![0x88];
        for (name, value) in
            core::iter::once(("content-type", "application/json")).chain(extra.iter().copied())
        {
            payload.push(0);
            payload.push(name.len() as u8);
            payload.extend_from_slice(name.as_bytes());
            payload.push(value.len() as u8);
            payload.extend_from_slice(value.as_bytes());
        }
        ResponseFrame { kind: 1, flags: if ended { 5 } else { 4 }, stream_identifier: 1, payload }
    }

    fn data(payload: &[u8], flags: u8) -> ResponseFrame {
        ResponseFrame { kind: 0, flags, stream_identifier: 1, payload: payload.to_vec() }
    }

    #[test]
    fn complete_fixed_or_unlengthened_bodies_use_the_shared_response_gate() {
        for fields in [vec![], vec![("content-length", "2")]] {
            let mut response = FiniteResponse::new();
            response.accept(&head(&fields, false)).unwrap();
            assert!(!response.stream_ended());
            response.accept(&data(b"{", 0)).unwrap();
            response.accept(&data(b"}", 1)).unwrap();
            assert!(response.stream_ended());
            let completed = response
                .finish_at_transport_end(
                    crate::selected_author_http2_frames::TransportEnd::for_test(),
                )
                .unwrap();
            assert_eq!(completed.status, 200);
            assert_eq!(completed.body, b"{}");
        }
        let mut response = FiniteResponse::new();
        response.accept(&head(&[], true)).unwrap();
        assert!(
            response
                .finish_at_transport_end(
                    crate::selected_author_http2_frames::TransportEnd::for_test()
                )
                .unwrap()
                .body
                .is_empty()
        );
    }

    #[test]
    fn invalid_head_policy_refuses_before_any_body_is_collected() {
        for fields in [
            vec![("content-encoding", "gzip")],
            vec![("content-type", "text/plain")],
            vec![("content-length", "2"), ("content-length", "2")],
            vec![("content-length", "-1")],
            vec![("content-length", "18446744073709551616")],
            vec![("trailer", "x")],
            vec![("retry-after", "1"), ("retry-after", "2")],
        ] {
            let mut response = FiniteResponse::new();
            assert!(response.accept(&head(&fields, false)).is_err());
            assert!(response.body.is_empty());
            assert!(response.accept(&data(b"x", 1)).is_err());
            assert!(
                response
                    .finish_at_transport_end(
                        crate::selected_author_http2_frames::TransportEnd::for_test()
                    )
                    .is_err()
            );
        }
        let mut response = FiniteResponse::new();
        response.maximum = 1;
        assert!(response.accept(&head(&[("content-length", "2")], false)).is_err());
        assert!(response.body.is_empty());
    }

    #[test]
    fn short_long_and_over_budget_bodies_never_finish() {
        for (bytes, maximum, content_length) in
            [(b"x".as_slice(), 2, "2"), (b"xxx", 3, "2"), (b"xxx", 2, "")]
        {
            let mut response = FiniteResponse::new();
            response.maximum = maximum;
            let fields = if content_length.is_empty() {
                vec![]
            } else {
                vec![("content-length", content_length)]
            };
            response.accept(&head(&fields, false)).unwrap();
            assert!(response.accept(&data(bytes, 1)).is_err());
            assert!(response.body.len() as u64 <= maximum);
            assert!(
                response
                    .finish_at_transport_end(
                        crate::selected_author_http2_frames::TransportEnd::for_test()
                    )
                    .is_err()
            );
        }
        let mut response = FiniteResponse::new();
        response.maximum = 2;
        response.accept(&head(&[], false)).unwrap();
        response.accept(&data(b"{}", 1)).unwrap();
        assert_eq!(
            response
                .finish_at_transport_end(
                    crate::selected_author_http2_frames::TransportEnd::for_test()
                )
                .unwrap()
                .body,
            b"{}"
        );
    }

    #[test]
    fn padding_is_not_content_but_is_returned_as_flow_credit() {
        let mut response = FiniteResponse::new();
        response.accept(&head(&[("content-length", "2")], false)).unwrap();
        let credit = response.accept(&data(&[1, b'{', b'}', 0], 9)).unwrap().unwrap();
        assert_eq!(&credit[0][9..], &[0, 0, 0, 4]);
        assert_eq!(
            response
                .finish_at_transport_end(
                    crate::selected_author_http2_frames::TransportEnd::for_test()
                )
                .unwrap()
                .body,
            b"{}"
        );
    }

    #[test]
    fn partial_headers_trailers_and_data_after_end_are_never_terminal_evidence() {
        let mut initial = head(&[], false);
        initial.flags = 0;
        let mut response = FiniteResponse::new();
        response.accept(&initial).unwrap();
        assert!(
            response
                .finish_at_transport_end(
                    crate::selected_author_http2_frames::TransportEnd::for_test()
                )
                .is_err()
        );
        for extra in [head(&[], true), data(b"extra", 0)] {
            let mut response = FiniteResponse::new();
            response.accept(&head(&[], true)).unwrap();
            assert!(response.accept(&extra).is_err());
            assert!(!response.stream_ended());
            assert!(
                response
                    .finish_at_transport_end(
                        crate::selected_author_http2_frames::TransportEnd::for_test()
                    )
                    .is_err()
            );
        }
        let mut response = FiniteResponse::new();
        response.accept(&head(&[], false)).unwrap();
        assert!(response.accept(&head(&[], true)).is_err());
        assert!(
            response
                .finish_at_transport_end(
                    crate::selected_author_http2_frames::TransportEnd::for_test()
                )
                .is_err()
        );
        assert!(FiniteResponse::new().accept(&data(b"x", 1)).is_err());
    }

    #[test]
    fn no_content_status_refuses_declared_lengths_and_nonempty_content() {
        let mut initial = head(&[], false);
        initial.payload[0] = 0x89;
        let mut response = FiniteResponse::new();
        response.accept(&initial).unwrap();
        assert!(response.accept(&data(b"x", 1)).is_err());
        let mut initial = head(&[("content-length", "0")], true);
        initial.payload[0] = 0x89;
        assert!(FiniteResponse::new().accept(&initial).is_err());
        let mut initial = head(&[], true);
        initial.payload[0] = 0x89;
        let mut response = FiniteResponse::new();
        response.accept(&initial).unwrap();
        assert_eq!(format!("{response:?}"), "FiniteResponse([redacted])");
        assert_eq!(
            response
                .finish_at_transport_end(
                    crate::selected_author_http2_frames::TransportEnd::for_test()
                )
                .unwrap()
                .status,
            204
        );
        for (length, valid) in [("0", true), ("1", false)] {
            let mut initial = head(&[("content-length", length)], true);
            initial.payload.splice(..1, [0x08, 3, b'2', b'0', b'5']);
            let mut response = FiniteResponse::new();
            assert_eq!(response.accept(&initial).is_ok(), valid);
            assert_eq!(
                response
                    .finish_at_transport_end(
                        crate::selected_author_http2_frames::TransportEnd::for_test()
                    )
                    .is_ok(),
                valid
            );
        }
    }
}
