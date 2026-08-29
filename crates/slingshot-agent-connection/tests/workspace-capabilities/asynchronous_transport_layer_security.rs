//! Probe for the asynchronous transport-layer-security capability.
//!
//! Requires the security handshake to run over an asynchronous connection under
//! a deadline of its own, separate from the deadline that bounds reaching the
//! peer, and to fail with a protocol error rather than a timeout when the peer
//! answers with bytes that are not a handshake.

use std::sync::Arc;
use std::time::Duration;

use rustls::crypto::ring;
use rustls::{ClientConfig, RootCertStore};
use rustls_pki_types::ServerName;
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::TlsConnector;

use crate::material;

/// Deadline that bounds reaching the peer.
const CONNECT_DEADLINE: Duration = Duration::from_millis(500);

/// Deadline that separately bounds the security handshake.
const HANDSHAKE_DEADLINE: Duration = Duration::from_millis(50);

/// Builds a connector over exactly one explicitly supplied root.
fn connector() -> TlsConnector {
    let mut store = RootCertStore::empty();
    store.add(material::certificate("author-root")).expect("the root is accepted");
    let configuration = ClientConfig::builder_with_provider(Arc::new(ring::default_provider()))
        .with_safe_default_protocol_versions()
        .expect("the protocol versions are supported")
        .with_root_certificates(store)
        .with_no_client_auth();
    TlsConnector::from(Arc::new(configuration))
}

#[test]
fn the_handshake_has_a_deadline_separate_from_reaching_the_peer() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("the runtime builds");
    runtime.block_on(async {
        let silent = TcpListener::bind("127.0.0.1:0").await.expect("the silent peer binds");
        let silent_address = silent.local_addr().expect("the peer reports its address");
        let holding = tokio::spawn(async move {
            let (accepted, _) = silent.accept().await.expect("a client arrives");
            tokio::time::sleep(CONNECT_DEADLINE).await;
            drop(accepted);
        });

        let stream = tokio::time::timeout(CONNECT_DEADLINE, TcpStream::connect(silent_address))
            .await
            .expect("reaching the peer stays within its own deadline")
            .expect("the connection is established");
        let name = ServerName::try_from("author.example.invalid").expect("the host name is valid");
        let elapsed =
            tokio::time::timeout(HANDSHAKE_DEADLINE, connector().connect(name.clone(), stream))
                .await;
        assert!(elapsed.is_err(), "the handshake deadline elapses on its own");
        holding.await.expect("the silent peer finishes");

        let rude = TcpListener::bind("127.0.0.1:0").await.expect("the rude peer binds");
        let rude_address = rude.local_addr().expect("the peer reports its address");
        let answering = tokio::spawn(async move {
            let (mut accepted, _) = rude.accept().await.expect("a client arrives");
            accepted.write_all(b"not a handshake\r\n").await.expect("the peer answers");
            accepted.shutdown().await.expect("the peer closes");
        });
        let stream = TcpStream::connect(rude_address).await.expect("the connection is established");
        let refused = tokio::time::timeout(CONNECT_DEADLINE, connector().connect(name, stream))
            .await
            .expect("the handshake finishes inside its deadline");
        assert!(refused.is_err(), "a peer that is not speaking the protocol is refused");
        answering.await.expect("the rude peer finishes");
    });
}
