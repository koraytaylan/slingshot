//! Byte-level IMS HTTP/1.1 transcripts and virtual-time deadline proofs.

use super::*;
struct Clock;
impl MonotonicClock for Clock {
    fn reading_milliseconds(&self) -> u64 {
        1000
    }
}

async fn run(wire: &[u8]) -> Result<IdentityManagementReceipt, ExchangeFailure> {
    let (client, mut peer) = tokio::io::duplex(4096);
    let exchange = exchange_http1(client, b"client_id=test", &Clock);
    let server = async {
        let mut request = Vec::new();
        while !request.ends_with(b"\r\n\r\n") {
            request.push(peer.read_u8().await.unwrap());
        }
        let request = String::from_utf8(request).unwrap();
        assert!(
            request
                .starts_with("POST /ims/exchange/jwt HTTP/1.1\r\nHost: ims-na1.adobelogin.com\r\n")
        );
        assert!(request.contains("Content-Type: application/x-www-form-urlencoded\r\n"));
        assert!(request.contains("Content-Length: 14\r\n"));
        assert!(!request.contains("Expect:") && !request.contains("Upgrade:"));
        let mut body = [0; 14];
        peer.read_exact(&mut body).await.unwrap();
        assert_eq!(&body, b"client_id=test");
        let _ = peer.write_all(wire).await;
        let _ = peer.shutdown().await;
        let mut extra = Vec::new();
        let _ = peer.read_to_end(&mut extra).await;
        assert!(extra.is_empty(), "a failed exchange retried its request");
    };
    let (result, ()) = tokio::join!(exchange, server);
    result
}
fn response(fields: &str, body: &[u8]) -> Vec<u8> {
    let mut wire =
        format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n{fields}\r\n").into_bytes();
    wire.extend_from_slice(body);
    wire
}

#[tokio::test]
async fn fixed_and_close_delimited_responses_preserve_exact_body() {
    for wire in [
        response("Content-Length: 2\r\n", b"{}"),
        response("", b"{}"),
        b"HTTP/1.1 200 OK\r\nContent-Type:\t application/json \t\r\nContent-Length: 2\r\n\r\n{}"
            .to_vec(),
    ] {
        let receipt = run(&wire).await.unwrap();
        assert_eq!(receipt.response.body, b"{}");
        assert_eq!(format!("{receipt:?}"), "IdentityManagementReceipt([redacted])");
    }
}

#[tokio::test]
async fn malformed_framing_partial_bodies_and_surplus_never_produce_receipts() {
    for wire in [
        response("Content-Length: 2\r\nContent-Length: 3\r\n", b"{}"),
        response("Content-Length: +2\r\n", b"{}"),
        response("Content-Length: 2\r\nTransfer-Encoding: chunked\r\n", b"{}"),
        response("Transfer-Encoding: gzip, chunked\r\n", b"{}"),
        response("Content-Length: 3\r\n", b"{}"),
        response("Content-Length: 1\r\n", b"{}"),
        response("Transfer-Encoding: chunked\r\n", b"2\r\n{\r\n0\r\n\r\n"),
        b"HTTP/1.1 200 OK\r\n folded: invalid\r\n\r\n".to_vec(),
        b"HTTP/1.1 200 OK\nContent-Length: 0\n\n".to_vec(),
    ] {
        assert_eq!(run(&wire).await.unwrap_err().code, Code::IdentityManagementTransportFailed);
    }
}

#[tokio::test]
async fn informational_redirect_media_and_trailer_checkpoints_are_terminal() {
    for (wire, expected) in [
        (
            response("Transfer-Encoding: chunked\r\n", b"1;name=value\r\n{\r\n1\r\n}\r\n0\r\n\r\n"),
            Code::IdentityManagementResponseTrailerRejected,
        ),
        (
            b"HTTP/1.1 100 Continue\r\n\r\nHTTP/1.1 200 OK\r\n\r\n".to_vec(),
            Code::IdentityManagementResponseStatusRejected,
        ),
        (
            b"HTTP/1.1 302 Redirect\r\nLocation: http://trap.invalid\r\n\r\n".to_vec(),
            Code::IdentityManagementRedirectRefused,
        ),
        (
            b"HTTP/1.1 401 Unauthorized\r\n\r\n".to_vec(),
            Code::IdentityManagementResponseStatusRejected,
        ),
        (response("Trailer: x-proof\r\n", b"{}"), Code::IdentityManagementResponseTrailerRejected),
        (
            response("Transfer-Encoding: chunked\r\n", b"0\r\nx-proof: no\r\n\r\n"),
            Code::IdentityManagementResponseTrailerRejected,
        ),
        (
            response("Content-Encoding: gzip\r\n", b"{}"),
            Code::IdentityManagementResponseMediaInvalid,
        ),
        (
            response("Content-Type: application/json\r\n", b"{}"),
            Code::IdentityManagementResponseMediaInvalid,
        ),
    ] {
        assert_eq!(run(&wire).await.unwrap_err().code, expected);
    }
}

#[tokio::test]
async fn decoded_fields_and_body_are_bounded_before_overflow_is_retained() {
    let limits = &ProfileAuthenticationContract::embedded().limits;
    let maximum = limits.maximum_identity_management_response_header_bytes as usize;
    assert!(run(&response(&format!("x: {}\r\n", "a".repeat(maximum - 1)), b"{}")).await.is_ok());
    assert_eq!(
        run(&response(&format!("x: {}\r\n", "a".repeat(maximum)), b"{}")).await.unwrap_err().code,
        Code::IdentityManagementResponseHeadLimitExceeded
    );
    let fields =
        "x: \r\n".repeat(limits.maximum_identity_management_response_header_count as usize);
    assert_eq!(
        run(&response(&fields, b"{}")).await.unwrap_err().code,
        Code::IdentityManagementResponseHeadLimitExceeded
    );
    let maximum = limits.maximum_identity_management_response_body_bytes as usize;
    assert_eq!(
        run(&response("", &vec![b'x'; maximum])).await.unwrap().response.body.len(),
        maximum
    );
    assert_eq!(
        run(&response("", &vec![b'x'; maximum + 1])).await.unwrap_err().code,
        Code::IdentityManagementResponseBodyLimitExceeded
    );
}

#[tokio::test(start_paused = true)]
async fn pending_write_head_and_body_have_distinct_deadlines() {
    let (client, _peer) = tokio::io::duplex(1);
    assert_eq!(
        exchange_http1(client, b"x", &Clock).await.unwrap_err().code,
        Code::IdentityManagementRequestWriteTimeout
    );
    let (client, mut peer) = tokio::io::duplex(4096);
    let server = async {
        let mut request = [0; 4096];
        assert!(peer.read(&mut request).await.unwrap() > 0);
        tokio::time::sleep(Duration::from_secs(1000)).await;
    };
    let client = async { exchange_http1(client, b"", &Clock).await.unwrap_err().code };
    tokio::pin!(server);
    let code = tokio::select! { code = client => code, _ = &mut server => panic!("header deadline did not fire") };
    assert_eq!(code, Code::IdentityManagementResponseHeaderTimeout);
    let (mut client, _peer) = tokio::io::duplex(1);
    let mut reader =
        BodyReader { stream: &mut client, total: Instant::now() + Duration::from_secs(1000) };
    assert_eq!(
        reader.byte().await.unwrap_err().code,
        Code::IdentityManagementResponseBodyIdleTimeout
    );
    reader.total = Instant::now();
    assert_eq!(
        reader.byte().await.unwrap_err().code,
        Code::IdentityManagementResponseBodyTotalTimeout
    );
}

#[tokio::test]
async fn cancelling_a_partial_response_drops_the_socket_without_a_receipt_or_retry() {
    let (client, mut peer) = tokio::io::duplex(4096);
    let mut exchange = Box::pin(exchange_http1(client, b"", &Clock));
    let server = async {
        let mut request = Vec::new();
        while !request.ends_with(b"\r\n\r\n") {
            request.push(peer.read_u8().await.unwrap());
        }
        peer.write_all(&response("Content-Length: 20\r\n", b"{\"partial-token\"")).await.unwrap();
    };
    tokio::select! {
        result = &mut exchange => panic!("partial response produced a result: {result:?}"),
        () = server => {},
    }
    use std::{
        future::Future,
        task::{Context, Waker},
    };
    assert!(exchange.as_mut().poll(&mut Context::from_waker(Waker::noop())).is_pending());
    drop(exchange);
    let mut extra = Vec::new();
    tokio::time::timeout(Duration::from_secs(1), peer.read_to_end(&mut extra))
        .await
        .unwrap()
        .unwrap();
    assert!(extra.is_empty());
}

#[tokio::test(start_paused = true)]
async fn total_deadline_expires_even_when_the_next_byte_is_already_ready() {
    let (mut client, mut peer) = tokio::io::duplex(16);
    let mut reader =
        BodyReader { stream: &mut client, total: Instant::now() + Duration::from_secs(1) };
    peer.write_all(b"ab").await.unwrap();
    assert_eq!(reader.byte().await.unwrap(), Some(b'a'));
    tokio::time::advance(Duration::from_secs(1)).await;
    assert_eq!(
        reader.byte().await.unwrap_err().code,
        Code::IdentityManagementResponseBodyTotalTimeout
    );
}
