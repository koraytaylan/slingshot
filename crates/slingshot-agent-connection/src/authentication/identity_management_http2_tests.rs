//! Joined IMS driver transcripts with flow stalls and virtual-time deadlines.

use super::*;
use crate::selected_author_http2::tests::{close, frame, handshake, receive};
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::io::AsyncReadExt;

struct Clock(AtomicU64);
impl MonotonicClock for Clock {
    fn reading_milliseconds(&self) -> u64 {
        self.0.fetch_add(100, Ordering::SeqCst)
    }
}
fn response_head(status: &[u8]) -> Vec<u8> {
    let mut block = vec![0x08, 3];
    block.extend_from_slice(status);
    block.extend_from_slice(&[0x0f, 16, 16]);
    block.extend_from_slice(b"application/json");
    frame(1, 4, 1, &block)
}

#[tokio::test]
async fn exact_post_waits_for_credit_and_finishes_only_after_peer_eof() {
    let clock = Clock(AtomicU64::new(0));
    let (client, mut peer) = tokio::io::duplex(256);
    let body = vec![b'x'; 20_000];
    let server = async {
        handshake(&mut peer, 7).await;
        let head = receive(&mut peer).await;
        assert_eq!((head.kind, head.flags, head.stream_identifier), (1, 4, 1));
        assert_eq!(&head.payload[..2], &[0x83, 0x87]);
        for expected in [
            b"ims-na1.adobelogin.com".as_slice(),
            b"/ims/exchange/jwt",
            b"application/x-www-form-urlencoded",
            b"20000",
        ] {
            assert!(head.payload.windows(expected.len()).any(|bytes| bytes == expected));
        }
        let first = receive(&mut peer).await;
        assert_eq!((first.kind, first.flags, first.payload.len()), (0, 0, 7));
        peer.write_all(&frame(8, 0, 1, &20_000_u32.to_be_bytes())).await.unwrap();
        let mut received = first.payload.clone();
        loop {
            let data = receive(&mut peer).await;
            assert_eq!(data.kind, 0);
            received.extend_from_slice(&data.payload);
            if data.flags & 1 != 0 {
                break;
            }
        }
        assert_eq!(received, body);
        peer.write_all(&response_head(b"200")).await.unwrap();
        peer.write_all(&frame(0, 1, 1, b"{}")).await.unwrap();
        close(&mut peer).await;
    };
    let (result, ()) = tokio::join!(exchange_http2(client, &body, &clock), server);
    assert_eq!(result.unwrap().response.body, b"{}");
    assert_eq!(clock.0.load(Ordering::SeqCst), 200, "anchor and receipt must each sample once");
}

#[tokio::test]
async fn early_complete_response_closes_the_unsent_body_without_retry() {
    let clock = Clock(AtomicU64::new(0));
    let (client, mut peer) = tokio::io::duplex(256);
    let server = async {
        handshake(&mut peer, 0).await;
        assert_eq!(receive(&mut peer).await.kind, 1);
        peer.write_all(&response_head(b"200")).await.unwrap();
        peer.write_all(&frame(0, 1, 1, b"{}")).await.unwrap();
        assert_eq!(receive(&mut peer).await.kind, 3, "unsent request must be reset, not retried");
        close(&mut peer).await;
    };
    let (result, ()) = tokio::join!(exchange_http2(client, b"secret-form", &clock), server);
    assert_eq!(result.unwrap().response.body, b"{}");
}

#[tokio::test(start_paused = true)]
async fn handshake_write_header_and_body_silence_have_distinct_deadlines() {
    for (phase, expected) in [
        ("handshake", Code::IdentityManagementResponseHeaderTimeout),
        ("write", Code::IdentityManagementRequestWriteTimeout),
        ("head", Code::IdentityManagementResponseHeaderTimeout),
        ("body", Code::IdentityManagementResponseBodyIdleTimeout),
    ] {
        let clock = Clock(AtomicU64::new(0));
        let (client, mut peer) = tokio::io::duplex(4096);
        let server = async {
            if phase != "handshake" {
                handshake(&mut peer, if phase == "write" { 0 } else { 65535 }).await;
                receive(&mut peer).await;
                if phase != "write" {
                    receive(&mut peer).await;
                }
                if phase == "body" {
                    peer.write_all(&response_head(b"200")).await.unwrap();
                }
            }
            std::future::pending::<()>().await;
        };
        let result = tokio::select! { result = exchange_http2(client, b"x", &clock) => result, () = server => unreachable!() };
        assert_eq!(result.unwrap_err().code, expected, "{phase}");
    }
}

#[tokio::test(start_paused = true)]
async fn control_frame_activity_cannot_extend_the_total_body_deadline() {
    let clock = Clock(AtomicU64::new(0));
    let (client, mut peer) = tokio::io::duplex(4096);
    let server = async {
        handshake(&mut peer, 65535).await;
        receive(&mut peer).await;
        peer.write_all(&response_head(b"200")).await.unwrap();
        let interval = ProfileAuthenticationContract::embedded()
            .limits
            .identity_management_response_body_idle_timeout_milliseconds
            / 2;
        loop {
            if peer.write_all(&frame(6, 0, 0, &[0; 8])).await.is_err() {
                std::future::pending::<()>().await;
            }
            tokio::time::sleep(Duration::from_millis(interval)).await;
        }
    };
    let result = tokio::select! { result = exchange_http2(client, b"", &clock) => result, () = server => unreachable!() };
    assert_eq!(result.unwrap_err().code, Code::IdentityManagementResponseBodyTotalTimeout);
}

#[tokio::test]
async fn rejection_codes_survive_the_network_driver() {
    for (status, code) in [
        (b"103", Code::IdentityManagementResponseStatusRejected),
        (b"302", Code::IdentityManagementRedirectRefused),
    ] {
        let clock = Clock(AtomicU64::new(0));
        let (client, mut peer) = tokio::io::duplex(4096);
        let server = async {
            handshake(&mut peer, 65535).await;
            receive(&mut peer).await;
            peer.write_all(&response_head(status)).await.unwrap();
            let mut extra = Vec::new();
            peer.read_to_end(&mut extra).await.unwrap();
            assert!(extra.is_empty());
        };
        let (result, ()) = tokio::join!(exchange_http2(client, b"", &clock), server);
        assert_eq!(result.unwrap_err().code, code);
    }
}

#[tokio::test]
async fn cancellation_drops_both_halves_without_background_request_work() {
    let clock = Clock(AtomicU64::new(0));
    let (client, mut peer) = tokio::io::duplex(4096);
    let mut exchange = Box::pin(exchange_http2(client, b"", &clock));
    let server = async {
        handshake(&mut peer, 65535).await;
        receive(&mut peer).await;
        peer.write_all(&response_head(b"200")).await.unwrap();
    };
    tokio::select! { result = &mut exchange => panic!("unexpected completion: {result:?}"), () = server => {} }
    assert!(
        exchange.as_mut().poll(&mut Context::from_waker(std::task::Waker::noop())).is_pending()
    );
    drop(exchange);
    let mut extra = Vec::new();
    tokio::time::timeout(Duration::from_secs(1), peer.read_to_end(&mut extra))
        .await
        .unwrap()
        .unwrap();
    assert!(extra.is_empty());
}
