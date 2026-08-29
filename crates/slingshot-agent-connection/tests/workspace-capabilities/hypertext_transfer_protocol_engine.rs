//! Probe for the protocol-engine capability.
//!
//! Requires a connection the caller drives itself, so nothing follows a
//! redirection, rewrites an encoding, or migrates a protocol behind its back.
//! It must surface every informational head before the final one, report the
//! presence and contents of a trailer section, and refuse an ambiguously framed
//! or truncated message instead of guessing.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use bytes::Bytes;
use http::{Request, StatusCode, Version};
use http_body_util::{BodyExt, Empty};
use hyper::body::Incoming;
use hyper_util::rt::TokioIo;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// Bytes a scripted peer reads before it answers.
const REQUEST_READ_LIMIT: usize = 4_096;

/// Starts a peer that reads one request and answers with exactly `script`.
async fn scripted_peer(script: &'static [u8]) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("the scripted peer binds");
    let address = listener.local_addr().expect("the peer reports its address");
    tokio::spawn(async move {
        let (mut accepted, _) = listener.accept().await.expect("a client arrives");
        let mut request = vec![0_u8; REQUEST_READ_LIMIT];
        let read = accepted.read(&mut request).await.expect("the request arrives");
        assert!(read > 0, "the client sent a request");
        accepted.write_all(script).await.expect("the script is written");
        accepted.shutdown().await.expect("the peer closes");
    });
    address
}

/// Sends one request over a connection the caller drives and returns the head.
async fn exchange(
    address: SocketAddr,
    informational: Option<Arc<Mutex<Vec<StatusCode>>>>,
) -> hyper::Result<http::Response<Incoming>> {
    let stream = TcpStream::connect(address).await.expect("the connection is established");
    let (mut sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
        .await
        .expect("the connection is ready");
    tokio::spawn(async move {
        let _finished = connection.await;
    });
    let mut request = Request::builder()
        .uri("/bin/querybuilder.json")
        .header("host", "author.example.invalid")
        .body(Empty::<Bytes>::new())
        .expect("the request builds");
    if let Some(recorded) = informational {
        hyper::ext::on_informational(&mut request, move |head| {
            recorded.lock().expect("the record is not poisoned").push(head.status());
        });
    }
    sender.send_request(request).await
}

#[test]
fn the_engine_surfaces_informational_heads_trailers_and_framing_failures() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("the runtime builds");
    runtime.block_on(async {
        let recorded = Arc::new(Mutex::new(Vec::new()));
        let address = scripted_peer(
            b"HTTP/1.1 103 Early Hints\r\nlink: </style.css>; rel=preload\r\n\r\n\
              HTTP/1.1 200 OK\r\ntransfer-encoding: chunked\r\ntrailer: x-digest\r\n\r\n\
              5\r\nhello\r\n0\r\nx-digest: abc\r\n\r\n",
        )
        .await;
        let response = exchange(address, Some(Arc::clone(&recorded))).await.expect("the exchange finishes");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.version(), Version::HTTP_11, "no protocol migration happened");
        assert_eq!(
            response.headers().get("trailer").and_then(|value| value.to_str().ok()),
            Some("x-digest"),
            "the head announces the trailer section"
        );
        let collected = response.into_body().collect().await.expect("the body collects");
        let trailers = collected.trailers().cloned().expect("the trailer section is present");
        assert_eq!(trailers.get("x-digest").and_then(|value| value.to_str().ok()), Some("abc"));
        assert_eq!(collected.to_bytes(), Bytes::from_static(b"hello"));
        assert_eq!(*recorded.lock().expect("the record is not poisoned"), vec![StatusCode::EARLY_HINTS]);

        let address = scripted_peer(b"HTTP/1.1 200 OK\r\ncontent-length: 5\r\n\r\nhello").await;
        let response = exchange(address, None).await.expect("the exchange finishes");
        assert!(response.headers().get("trailer").is_none(), "no trailer section is announced");
        let collected = response.into_body().collect().await.expect("the body collects");
        assert!(collected.trailers().is_none(), "a counted body carries no trailer section");

        let address =
            scripted_peer(b"HTTP/1.1 200 OK\r\ntransfer-encoding: chunked\r\n\r\n5\r\nhello\r\n0\r\n\r\n").await;
        let response = exchange(address, None).await.expect("the exchange finishes");
        let collected = response.into_body().collect().await.expect("the body collects");
        assert!(collected.trailers().is_none(), "an empty trailer section carries no fields");
        assert_eq!(collected.to_bytes(), Bytes::from_static(b"hello"));

        let address =
            scripted_peer(b"HTTP/1.1 200 OK\r\ncontent-length: 5\r\ncontent-length: 6\r\n\r\nhello").await;
        let refused = exchange(address, None).await;
        assert!(refused.is_err(), "two conflicting body lengths must be refused");

        let address =
            scripted_peer(b"HTTP/1.1 200 OK\r\ntransfer-encoding: chunked\r\n\r\nzz\r\nhello\r\n").await;
        let response = exchange(address, None).await.expect("the head arrives");
        assert!(
            response.into_body().collect().await.is_err(),
            "a chunk whose length is not a number must be refused"
        );

        let address = scripted_peer(b"HTTP/1.1 200 OK\r\ncontent-length: 10\r\n\r\nhel").await;
        let response = exchange(address, None).await.expect("the head arrives");
        assert!(response.into_body().collect().await.is_err(), "a truncated body must be refused");

        let address =
            scripted_peer(b"HTTP/1.1 302 Found\r\nlocation: /elsewhere\r\ncontent-length: 0\r\n\r\n").await;
        let response = exchange(address, None).await.expect("the exchange finishes");
        assert_eq!(response.status(), StatusCode::FOUND, "the redirection is not followed");

        let address = scripted_peer(
            b"HTTP/1.1 200 OK\r\ncontent-encoding: gzip\r\ncontent-length: 4\r\n\r\n\x1f\x8b\x08\x00",
        )
        .await;
        let response = exchange(address, None).await.expect("the exchange finishes");
        let body = response.into_body().collect().await.expect("the body collects").to_bytes();
        assert_eq!(body, Bytes::from_static(b"\x1f\x8b\x08\x00"), "the body is not decompressed");
    });
}
