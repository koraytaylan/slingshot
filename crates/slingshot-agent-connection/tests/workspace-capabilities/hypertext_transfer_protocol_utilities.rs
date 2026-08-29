//! Probe for the protocol-utilities capability.
//!
//! Requires the adapters that let the protocol engine run on the asynchronous
//! runtime: a stream adapter for the engine's byte traits and an executor the
//! multiplexed protocol drives its tasks with.

use bytes::Bytes;
use http::{Request, Response, StatusCode, Version};
use http_body_util::{BodyExt, Full};
use hyper::service::service_fn;
use hyper_util::rt::{TokioExecutor, TokioIo};
use tokio::net::{TcpListener, TcpStream};

#[test]
fn the_adapters_run_the_multiplexed_protocol_over_the_runtime() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("the runtime builds");
    runtime.block_on(async {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("the server binds");
        let address = listener.local_addr().expect("the server reports its address");
        tokio::spawn(async move {
            let (accepted, _) = listener.accept().await.expect("a client arrives");
            let service = service_fn(|request: Request<hyper::body::Incoming>| async move {
                let path = request.uri().path().to_owned();
                Ok::<_, hyper::Error>(
                    Response::builder()
                        .status(StatusCode::OK)
                        .body(Full::new(Bytes::from(path)))
                        .expect("the response builds"),
                )
            });
            hyper::server::conn::http2::Builder::new(TokioExecutor::new())
                .serve_connection(TokioIo::new(accepted), service)
                .await
                .expect("the connection is served");
        });

        let stream = TcpStream::connect(address).await.expect("the connection is established");
        let (mut sender, connection) =
            hyper::client::conn::http2::handshake(TokioExecutor::new(), TokioIo::new(stream))
                .await
                .expect("the multiplexed connection is ready");
        tokio::spawn(async move {
            let _finished = connection.await;
        });

        let request = Request::builder()
            .uri("http://author.example.invalid/bin/querybuilder.json")
            .body(Full::<Bytes>::new(Bytes::new()))
            .expect("the request builds");
        let response = sender.send_request(request).await.expect("the exchange finishes");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.version(), Version::HTTP_2, "the multiplexed protocol was negotiated");
        let body = response.into_body().collect().await.expect("the body collects").to_bytes();
        assert_eq!(body, Bytes::from_static(b"/bin/querybuilder.json"));
    });
}
