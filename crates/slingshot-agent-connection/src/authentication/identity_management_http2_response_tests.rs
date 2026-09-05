//! IMS response proofs through the real wire gate, HPACK and body assembler.

use super::*;
use crate::selected_author_http2_frames::{FrameRead, ResponseFrameReader};

fn frame(kind: u8, flags: u8, payload: &[u8]) -> Vec<u8> {
    let size = (payload.len() as u32).to_be_bytes();
    let mut wire =
        vec![size[1], size[2], size[3], kind, flags, 0, 0, 0, if kind == 4 { 0 } else { 1 }];
    wire.extend_from_slice(payload);
    wire
}
fn literal(wire: &mut Vec<u8>, bytes: &[u8]) {
    assert!(bytes.len() < 127);
    wire.push(bytes.len() as u8);
    wire.extend_from_slice(bytes);
}
fn head(status: u16, fields: &[(&str, &str)]) -> Vec<u8> {
    let mut block = vec![0x08];
    literal(&mut block, status.to_string().as_bytes());
    for (name, value) in
        [("content-type", "application/json")].into_iter().chain(fields.iter().copied())
    {
        block.push(0x40);
        literal(&mut block, name.as_bytes());
        literal(&mut block, value.as_bytes());
    }
    block
}
async fn run(frames: Vec<Vec<u8>>) -> Result<DecodedResponse, ExchangeFailure> {
    let mut wire = frame(4, 0, &[]);
    for frame in frames {
        wire.extend_from_slice(&frame);
    }
    let mut input = wire.as_slice();
    let mut reader = ResponseFrameReader::with_header_policy(maximum_head(), true);
    let mut response = IdentityManagementHttp2Response::new();
    loop {
        match reader.read_next(&mut input).await {
            Ok(FrameRead::Frame(frame)) if frame.kind == 4 => {}
            Ok(FrameRead::Frame(frame)) => {
                response.accept(&frame)?;
            }
            Ok(FrameRead::End(end)) => return response.finish_at_transport_end(end),
            Err(_) => {
                return Err(ExchangeFailure::new(if reader.header_limit_exceeded() {
                    Code::IdentityManagementResponseHeadLimitExceeded
                } else {
                    Code::IdentityManagementTransportFailed
                }));
            }
        }
    }
}

#[tokio::test]
async fn oversized_announced_literal_is_a_head_limit_before_any_payload() {
    for huffman in [0, 0x80] {
        for name in [true, false] {
            let mut block = vec![0x88, 0];
            if !name {
                block.extend_from_slice(&[1, b'x']);
            }
            block.push(huffman | 127);
            let mut remaining = maximum_head() + 1 - 127;
            while remaining >= 128 {
                block.push((remaining as u8 & 127) | 128);
                remaining >>= 7;
            }
            block.push(remaining as u8);
            for split in 0..=block.len() {
                let error = run(vec![frame(1, 0, &block[..split]), frame(9, 4, &block[split..])])
                    .await
                    .unwrap_err();
                assert_eq!(error.code, Code::IdentityManagementResponseHeadLimitExceeded);
            }
        }
    }
}

#[tokio::test]
async fn split_headers_padding_and_complete_eof_preserve_exact_content() {
    let block = head(200, &[("content-length", "2")]);
    for split in 0..=block.len() {
        let response = run(vec![
            frame(1, 0, &block[..split]),
            frame(9, 4, &block[split..]),
            frame(0, 9, &[2, b'{', b'}', 0, 0]),
        ])
        .await
        .unwrap();
        assert_eq!(response.head.status, 200);
        assert_eq!(response.body, b"{}");
        assert!(response.trailer.is_none());
    }
    let mut response = IdentityManagementHttp2Response::new();
    response
        .accept(&ResponseFrame { kind: 1, flags: 4, stream_identifier: 1, payload: block })
        .unwrap();
    let credit = response
        .accept(&ResponseFrame {
            kind: 0,
            flags: 9,
            stream_identifier: 1,
            payload: vec![2, b'{', b'}', 0, 0],
        })
        .unwrap()
        .unwrap();
    assert_eq!(&credit[0][9..], &5_u32.to_be_bytes());
    assert!(response.stream_ended());
}

#[tokio::test]
async fn informational_redirect_media_and_trailer_failures_keep_exact_codes() {
    for (status, fields, expected) in [
        (100, vec![], Code::IdentityManagementResponseStatusRejected),
        (103, vec![], Code::IdentityManagementResponseStatusRejected),
        (302, vec![("location", "https://trap.invalid")], Code::IdentityManagementRedirectRefused),
        (401, vec![], Code::IdentityManagementResponseStatusRejected),
        (200, vec![("content-encoding", "gzip")], Code::IdentityManagementResponseMediaInvalid),
        (200, vec![("trailer", "x")], Code::IdentityManagementResponseTrailerRejected),
    ] {
        assert_eq!(
            run(vec![frame(1, 4, &head(status, &fields))]).await.unwrap_err().code,
            expected
        );
    }
    for trailer in [vec![], vec![0xbe]] {
        assert_eq!(
            run(vec![
                frame(1, 4, &head(200, &[("x", "y")])),
                frame(0, 0, b"{}"),
                frame(1, 5, &trailer)
            ])
            .await
            .unwrap_err()
            .code,
            Code::IdentityManagementResponseTrailerRejected
        );
    }
}

#[tokio::test]
async fn framing_errors_and_partial_messages_never_finish() {
    let block = head(200, &[]);
    for frames in [
        vec![frame(1, 4, &head(200, &[("content-length", "3")])), frame(0, 1, b"{}")],
        vec![frame(1, 4, &head(200, &[("content-length", "1")])), frame(0, 1, b"{}")],
        vec![frame(1, 4, &head(200, &[("content-length", "1"), ("content-length", "2")]))],
        vec![frame(1, 5, &head(200, &[("content-length", "1")]))],
        vec![frame(1, 4, &block), frame(0, 0, b"{}")],
        vec![frame(1, 0, &block[..2]), frame(0, 1, b"{}")],
        vec![frame(1, 4, &block), frame(0, 1, b"{}"), frame(0, 1, b"x")],
        vec![frame(1, 4, &block), frame(1, 4, &[])],
        vec![frame(1, 4, &block), frame(1, 5, &[0x88])],
    ] {
        assert_eq!(run(frames).await.unwrap_err().code, Code::IdentityManagementTransportFailed);
    }
}

#[tokio::test]
async fn body_exact_limit_and_next_byte_have_no_partial_receipt() {
    for extra in [0, 1] {
        let body = vec![b'x'; maximum_body() as usize + extra];
        let mut frames = vec![frame(1, 4, &head(200, &[]))];
        let mut consumed = 0;
        for bytes in body.chunks(16_384) {
            consumed += bytes.len();
            frames.push(frame(0, u8::from(consumed == body.len()), bytes));
        }
        let result = run(frames).await;
        if extra == 0 {
            assert_eq!(result.unwrap().body.len(), body.len());
        } else {
            assert_eq!(result.unwrap_err().code, Code::IdentityManagementResponseBodyLimitExceeded);
        }
    }
}

#[tokio::test]
async fn route_frame_policy_preserves_default_rejection_and_classifies_bounds() {
    for allow in [false, true] {
        let mut reader = ResponseFrameReader::with_header_policy(100, allow);
        reader.read(&mut frame(4, 0, &[]).as_slice()).await.unwrap();
        reader.read(&mut frame(1, 4, &[0x88]).as_slice()).await.unwrap();
        assert_eq!(reader.read(&mut frame(1, 5, &[]).as_slice()).await.is_ok(), allow);
    }
    let mut reader = ResponseFrameReader::with_header_policy(0, true);
    reader.read(&mut frame(4, 0, &[]).as_slice()).await.unwrap();
    let wire = frame(1, 4, &[0x88]);
    let mut input = wire.as_slice();
    assert!(reader.read(&mut input).await.is_err());
    assert!(reader.header_limit_exceeded());
    assert_eq!(input, &[0x88], "over-bound payload was read before refusal");
}

#[test]
fn failures_are_poisoned_and_do_not_expose_body_or_change_code() {
    let mut response = IdentityManagementHttp2Response::new();
    let frame = ResponseFrame { kind: 1, flags: 4, stream_identifier: 1, payload: head(302, &[]) };
    let first = response.accept(&frame).unwrap_err();
    assert_eq!(first.code, Code::IdentityManagementRedirectRefused);
    assert_eq!(response.accept(&frame).unwrap_err(), first);
    assert!(!response.head_complete() && !response.stream_ended());
    assert_eq!(format!("{response:?}"), "IdentityManagementHttp2Response([redacted])");
    assert_eq!(response.finish_at_transport_end(TransportEnd::for_test()).unwrap_err(), first);
}

#[tokio::test]
async fn complete_section_accounting_precedes_status_and_trailer_refusal() {
    let count = ProfileAuthenticationContract::embedded()
        .limits
        .maximum_identity_management_response_header_count as usize;
    let mut oversized_head = head(302, &[("x", "y")]);
    oversized_head.extend(std::iter::repeat_n(0xbe, count));
    assert_eq!(
        run(vec![frame(1, 4, &oversized_head)]).await.unwrap_err().code,
        Code::IdentityManagementResponseHeadLimitExceeded
    );
    assert_eq!(
        run(vec![frame(1, 4, &head(200, &[("x", "y")])), frame(1, 5, &vec![0xbe; count + 1])])
            .await
            .unwrap_err()
            .code,
        Code::IdentityManagementResponseHeadLimitExceeded
    );
    assert_eq!(
        run(vec![frame(1, 4, &head(200, &[("x", "y")])), frame(1, 1, &[]), frame(9, 4, &[0xbe])])
            .await
            .unwrap_err()
            .code,
        Code::IdentityManagementResponseTrailerRejected
    );
    let response = run(vec![
        frame(1, 4, &head(200, &[("content-length", "2"), ("content-length", "2")])),
        frame(0, 1, b"{}"),
    ])
    .await
    .unwrap();
    assert_eq!(response.body, b"{}");
}
