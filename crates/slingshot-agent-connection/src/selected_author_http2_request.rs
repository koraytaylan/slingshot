//! HTTP/2 request heads below the same immutable selected-author URI gate.
//! Encoding a head neither connects nor grants permission to submit an operation.
//! The eventual driver must supply protocol negotiation, deadlines and durable
//! first-send/recovery authority before writing these bytes.

use crate::authentication::environment_provider::RequestAuthentication;
use crate::selected_author_http::{FiniteHttpFailure, prepare_request_uri};
use crate::selected_author_transport::SelectedAuthorTransport;
use http::{HeaderMap, HeaderValue, Method};

const FRAME_BYTES: usize = 16_384;

/// Private request head encoded with no dynamic-table insertion, including
/// credentials, identity headers and query-bearing pseudo-fields.
pub struct EncodedRequestHead {
    block: Vec<u8>,
    body_empty: bool,
}

impl core::fmt::Debug for EncodedRequestHead {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("EncodedRequestHead([redacted])")
    }
}

impl EncodedRequestHead {
    /// Serializes HEADERS followed by CONTINUATION frames on stream 1. The
    /// receiving peer's initial minimum frame allowance is never exceeded.
    /// END_STREAM belongs on the first HEADERS only for an empty request body;
    /// END_HEADERS belongs only on the final fragment.
    pub fn frames(&self) -> impl Iterator<Item = Vec<u8>> + '_ {
        let fragments = self.block.len().div_ceil(FRAME_BYTES);
        self.block.chunks(FRAME_BYTES).enumerate().map(move |(index, fragment)| {
            let length = (fragment.len() as u32).to_be_bytes();
            let flags = if index + 1 == fragments { 4 } else { 0 }
                | if index == 0 && self.body_empty { 1 } else { 0 };
            let mut frame = Vec::with_capacity(9 + fragment.len());
            frame.extend_from_slice(&[
                length[1],
                length[2],
                length[3],
                if index == 0 { 1 } else { 9 },
                flags,
                0,
                0,
                0,
                1,
            ]);
            frame.extend_from_slice(fragment);
            frame
        })
    }
}

impl SelectedAuthorTransport {
    /// Builds only a selected-origin GET/POST head. No arbitrary URI or caller
    /// framing/authentication override is accepted. Data frames are deliberately
    /// left to the send-window-aware driver, not blindly serialized here.
    pub fn encode_http2_request_head(
        &self,
        method: Method,
        segments: &[&str],
        query: &[(&str, &str)],
        authentication: &RequestAuthentication,
        fields: &HeaderMap,
        body: &[u8],
    ) -> Result<EncodedRequestHead, FiniteHttpFailure> {
        self.require_authentication(authentication).map_err(|_| FiniteHttpFailure::Request)?;
        if !matches!(method, Method::GET | Method::POST) {
            return Err(FiniteHttpFailure::Request);
        }
        let uri = prepare_request_uri(self, segments, query, fields, body)?;
        for value in fields.values() {
            if value.as_bytes().first().is_some_and(|byte| matches!(byte, b' ' | b'\t'))
                || value.as_bytes().last().is_some_and(|byte| matches!(byte, b' ' | b'\t'))
            {
                return Err(FiniteHttpFailure::Request);
            }
        }
        let mut block = Vec::new();
        field(&mut block, b":method", method.as_str().as_bytes());
        field(
            &mut block,
            b":scheme",
            uri.scheme_str().ok_or(FiniteHttpFailure::Request)?.as_bytes(),
        );
        field(
            &mut block,
            b":authority",
            uri.authority().ok_or(FiniteHttpFailure::Request)?.as_str().as_bytes(),
        );
        field(
            &mut block,
            b":path",
            uri.path_and_query().ok_or(FiniteHttpFailure::Request)?.as_str().as_bytes(),
        );
        field(&mut block, b"accept-encoding", b"identity");
        field(&mut block, b"content-length", body.len().to_string().as_bytes());
        for (name, value) in fields {
            field(&mut block, name.as_str().as_bytes(), value.as_bytes());
        }
        authentication.lend_value_bytes(|value| {
            HeaderValue::from_bytes(value).map_err(|_| FiniteHttpFailure::Request)?;
            field(&mut block, b"authorization", value);
            Ok::<(), FiniteHttpFailure>(())
        })?;
        Ok(EncodedRequestHead { block, body_empty: body.is_empty() })
    }
}

fn field(output: &mut Vec<u8>, name: &[u8], value: &[u8]) {
    // Literal never-indexed, with a literal name. Huffman encoding is optional;
    // raw strings avoid a second retained representation of private values.
    output.push(0x10);
    string(output, name);
    string(output, value);
}

fn string(output: &mut Vec<u8>, bytes: &[u8]) {
    if bytes.len() < 127 {
        output.push(bytes.len() as u8);
    } else {
        output.push(127);
        let mut remaining = bytes.len() - 127;
        while remaining >= 128 {
            output.push((remaining as u8 & 127) | 128);
            remaining >>= 7;
        }
        output.push(remaining as u8);
    }
    output.extend_from_slice(bytes);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fragments_have_exact_boundaries_and_end_flags_without_data_frames() {
        for length in [1, FRAME_BYTES, FRAME_BYTES + 1, FRAME_BYTES * 2] {
            for body_empty in [true, false] {
                let request = EncodedRequestHead { block: vec![b'x'; length], body_empty };
                let frames: Vec<_> = request.frames().collect();
                let mut reconstructed = Vec::new();
                for (index, frame) in frames.iter().enumerate() {
                    let payload_length = usize::from(frame[0]) << 16
                        | usize::from(frame[1]) << 8
                        | usize::from(frame[2]);
                    assert_eq!(payload_length, frame.len() - 9);
                    assert!(payload_length <= FRAME_BYTES);
                    assert_eq!(frame[3], if index == 0 { 1 } else { 9 });
                    assert_eq!(frame[4] & 4 != 0, index + 1 == frames.len());
                    assert_eq!(frame[4] & 1 != 0, index == 0 && body_empty);
                    assert_eq!(&frame[5..9], &[0, 0, 0, 1]);
                    reconstructed.extend_from_slice(&frame[9..]);
                }
                assert_eq!(reconstructed, request.block);
                assert_eq!(format!("{request:?}"), "EncodedRequestHead([redacted])");
            }
        }
    }

    #[test]
    fn literal_lengths_and_never_indexed_marker_match_hpack_wire_examples() {
        let mut bytes = Vec::new();
        field(&mut bytes, b"x", b"abc");
        assert_eq!(bytes, b"\x10\x01x\x03abc");
        let mut bytes = Vec::new();
        string(&mut bytes, &vec![b'x'; 1337]);
        assert_eq!(&bytes[..3], &[127, 186, 9]);
        assert_eq!(&bytes[3..], &vec![b'x'; 1337]);
    }
}
