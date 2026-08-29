//! Probe for the asynchronous-runtime capability.
//!
//! Requires a multi-thread runtime, a loopback endpoint, a deadline that
//! elapses without a fixed sleep, and a child process the runtime can wait for.

use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// Length of the message this probe echoes over the endpoint.
const PROBE_MESSAGE_LENGTH: usize = 5;

/// Deadline used to prove that a timeout elapses rather than blocking forever.
const ELAPSING_DEADLINE: Duration = Duration::from_millis(25);

#[test]
fn the_runtime_serves_a_loopback_endpoint_and_honors_a_deadline() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("the multi-thread runtime builds");
    runtime.block_on(async {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("the endpoint binds");
        let address = listener.local_addr().expect("the endpoint reports its address");
        let server = tokio::spawn(async move {
            let (mut accepted, _) = listener.accept().await.expect("a client arrives");
            let mut received = [0_u8; PROBE_MESSAGE_LENGTH];
            accepted.read_exact(&mut received).await.expect("the request arrives");
            accepted.write_all(&received).await.expect("the response is written");
        });
        let mut client = TcpStream::connect(address).await.expect("the client connects");
        client.write_all(b"probe").await.expect("the request is written");
        let mut echoed = [0_u8; PROBE_MESSAGE_LENGTH];
        client.read_exact(&mut echoed).await.expect("the response arrives");
        assert_eq!(&echoed, b"probe");
        server.await.expect("the server task finishes");

        let (idle, _keep) = tokio::io::duplex(1);
        let elapsed = tokio::time::timeout(ELAPSING_DEADLINE, read_forever(idle)).await;
        assert!(elapsed.is_err(), "the deadline must elapse");

        let status =
            tokio::process::Command::new(std::env::current_exe().expect("the test executable"))
                .arg("--list")
                .output()
                .await
                .expect("the child process runs");
        assert!(status.status.success(), "the runtime waits for a child process");
    });
}

/// Reads from a stream that never produces a byte.
async fn read_forever(mut stream: tokio::io::DuplexStream) {
    let mut byte = [0_u8; 1];
    let _unused = stream.read_exact(&mut byte).await;
}
