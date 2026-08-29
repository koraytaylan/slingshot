//! Assertions for the local server of one owned runtime namespace.
//!
//! Every assertion runs against a real endpoint inside an injected temporary
//! runtime root. The deadline assertions run on a paused runtime clock, so the
//! exact values the foundation contract declares are proved without waiting
//! them out in wall-clock time.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use slingshot_daemon::local_server::{self, LocalListener};
use slingshot_daemon::ownership::{Acquisition, DaemonOwnership};
use slingshot_daemon::platform_runtime::endpoint::{self, EndpointAddress};
use slingshot_daemon::platform_runtime::locks::OwnerLock;
use slingshot_daemon::platform_runtime::{current_user, readiness};
use slingshot_daemon::runtime_namespace::RuntimeNamespace;
use slingshot_daemon::service::DaemonService;
use slingshot_local_protocol::envelope::{
    ControlRequest, ControlResponse, LIMIT_EXCEEDED_CODE, MALFORMED_REQUEST_CODE,
    METHOD_NOT_FOUND_CODE, ResponseOutcome, STALE_DAEMON_INSTANCE_CODE,
    UNSUPPORTED_CONTROL_VERSION_CODE,
};
use slingshot_local_protocol::foundation_contract::FoundationContract;
use slingshot_local_protocol::framing;
use slingshot_local_protocol::ping::{PING_METHOD, PingResult, STOP_METHOD};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio_util::sync::CancellationToken;

/// Profile the assertions name their target with.
const PROFILE: &str = "local";

/// Environment the assertions name their target with.
const ENVIRONMENT: &str = "author";

/// Clients one correlation assertion releases at once.
const CONCURRENT_CLIENT_COUNT: usize = 16;

/// Ways a stalled peer can withhold its frame.
const STALLED_PEER_KINDS: usize = 3;

/// The stalled peer that sends a declared length and part of its payload.
const PARTIAL_PAYLOAD_PEER: usize = 2;

/// One running daemon and everything a test needs to reach it.
struct RunningDaemon {
    address: EndpointAddress,
    shutdown: CancellationToken,
    served: tokio::task::JoinHandle<()>,
    service: Arc<DaemonService>,
    root: PathBuf,
    digest: String,
}

/// Creates an injected temporary runtime root that no other assertion shares.
///
/// The name is short on purpose: a Unix domain socket address is bounded, the
/// foundation contract records that bound, and the namespace digest takes most
/// of it. A runtime root that leaves no room is a real defect.
fn temporary_runtime_root(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("s{}{name}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    current_user::create_owner_only_directory(&root).expect("the runtime root is created");
    root
}

/// Starts one daemon for a target inside an injected runtime root.
async fn start_daemon(root: &Path, environment: &str) -> RunningDaemon {
    let contract = FoundationContract::embedded();
    let namespace = RuntimeNamespace::name(&contract, root, PROFILE, environment)
        .expect("the target names a namespace");
    let digest = namespace.digest().to_owned();
    let Acquisition::Owned(owned) =
        DaemonOwnership::acquire(&contract, namespace).expect("the runtime state is readable")
    else {
        panic!("the namespace must be free");
    };
    let address =
        endpoint::endpoint_address(&contract, root, &digest).expect("the endpoint is named");
    assert_eq!(
        readiness::read(root, &digest).expect("the record is readable"),
        None,
        "readiness is absent before the endpoint is bound"
    );
    let mut listener = LocalListener::bind(&address).expect("the endpoint binds");
    let mut service = DaemonService::new(contract.clone(), *owned);
    service
        .ownership_mut()
        .publish_readiness(&contract, &address.display())
        .expect("readiness publishes");
    let service = Arc::new(service);
    let shutdown = CancellationToken::new();
    let served = tokio::spawn({
        let service = Arc::clone(&service);
        let shutdown = shutdown.clone();
        async move {
            local_server::serve(service, &mut listener, shutdown)
                .await
                .expect("the endpoint serves");
            listener.remove();
        }
    });
    RunningDaemon { address, shutdown, served, service, root: root.to_path_buf(), digest }
}

/// Connects one client to a running daemon.
async fn connect(address: &EndpointAddress) -> UnixStream {
    let EndpointAddress::UnixDomainSocket(path) = address;
    UnixStream::connect(path).await.expect("the client connects")
}

/// Renders one request as a frame.
fn frame(
    contract: &FoundationContract,
    identifier: &str,
    method: &str,
    arguments: serde_json::Value,
) -> Vec<u8> {
    let request = ControlRequest {
        control_version: contract.control.version,
        request_identifier: identifier.to_owned(),
        method: method.to_owned(),
        arguments,
    };
    let payload = serde_json::to_vec(&request).expect("the request renders");
    framing::render(&contract.framing, &payload).expect("the request frames")
}

/// Sends one already-rendered frame and reads the response.
async fn exchange(address: &EndpointAddress, request: &[u8]) -> ControlResponse {
    let contract = FoundationContract::embedded();
    let mut stream = connect(address).await;
    stream.write_all(request).await.expect("the request is written");
    let payload = local_server::read_frame(&mut stream, &contract, true)
        .await
        .expect("the response arrives")
        .expect("the response is a whole frame");
    serde_json::from_slice(&payload).expect("the response reads")
}

/// Sends one ping and returns its result.
async fn ping(address: &EndpointAddress, identifier: &str) -> PingResult {
    let contract = FoundationContract::embedded();
    let response =
        exchange(address, &frame(&contract, identifier, PING_METHOD, serde_json::json!({}))).await;
    assert_eq!(response.outcome, ResponseOutcome::Success, "{response:?}");
    assert_eq!(response.request_identifier, identifier);
    serde_json::from_value(response.result.expect("a served ping carries a result"))
        .expect("the ping result reads")
}

/// Returns a nonce of the same shape that is never the live one.
fn stale_nonce(live: &str) -> String {
    let first = if live.starts_with('a') { 'b' } else { 'a' };
    let stale = format!("{first}{}", &live[1..]);
    assert_ne!(stale, live, "a stale nonce must differ from the live one");
    stale
}

/// Stops one daemon and waits for its accept loop to finish.
async fn finish(daemon: RunningDaemon) {
    daemon.shutdown.cancel();
    daemon.served.await.expect("the accept loop finishes");
    drop(daemon.service);
    std::fs::remove_dir_all(&daemon.root).ok();
}

#[tokio::test]
async fn readiness_is_present_only_once_the_endpoint_answers_a_ping() {
    let root = temporary_runtime_root("r");
    let daemon = start_daemon(&root, ENVIRONMENT).await;
    let result = ping(&daemon.address, "one").await;
    let record = readiness::read(&root, &daemon.digest)
        .expect("the record is readable")
        .expect("readiness is published");
    assert_eq!(record.readiness_nonce, result.readiness_nonce);
    assert_eq!(record.endpoint_display, daemon.address.display());
    assert_eq!(result.profile, PROFILE);
    assert_eq!(result.environment, ENVIRONMENT);
    assert!(result.supported_operation_protocol_versions.is_empty());
    finish(daemon).await;
}

#[tokio::test]
async fn concurrent_connections_receive_correctly_correlated_responses() {
    let root = temporary_runtime_root("c");
    let daemon = start_daemon(&root, ENVIRONMENT).await;
    let identifiers: Vec<String> =
        (0..CONCURRENT_CLIENT_COUNT).map(|index| format!("client-{index}")).collect();
    let mut pending = Vec::new();
    for identifier in &identifiers {
        let address = daemon.address.clone();
        let identifier = identifier.clone();
        pending.push(tokio::spawn(async move {
            let result = ping(&address, &identifier).await;
            (identifier, result.readiness_nonce, result.process_identifier)
        }));
    }
    let mut seen = Vec::new();
    for handle in pending {
        let (identifier, nonce, process_identifier) = handle.await.expect("the client finishes");
        assert_eq!(nonce, daemon.service.ownership().readiness_nonce());
        assert_eq!(process_identifier, std::process::id());
        seen.push(identifier);
    }
    seen.sort();
    let mut expected = identifiers;
    expected.sort();
    assert_eq!(seen, expected, "every client received its own response");
    finish(daemon).await;
}

#[tokio::test]
async fn a_refused_request_leaves_the_server_available_for_the_next_ping() {
    let contract = FoundationContract::embedded();
    let root = temporary_runtime_root("f");
    let daemon = start_daemon(&root, ENVIRONMENT).await;

    let malformed =
        framing::render(&contract.framing, b"{\"control_version\":").expect("it frames");
    let response = exchange(&daemon.address, &malformed).await;
    assert_eq!(response.error.expect("a refusal carries an error").code, MALFORMED_REQUEST_CODE);

    let oversized_method = frame(
        &contract,
        "method",
        &"m".repeat(contract.names.method_bytes as usize + 1),
        serde_json::json!({}),
    );
    let response = exchange(&daemon.address, &oversized_method).await;
    assert_eq!(response.error.expect("a refusal carries an error").code, LIMIT_EXCEEDED_CODE);

    let mut mismatched = ControlRequest {
        control_version: contract.control.version + 1,
        request_identifier: "version".to_owned(),
        method: PING_METHOD.to_owned(),
        arguments: serde_json::json!({}),
    };
    let payload = serde_json::to_vec(&mismatched).expect("the request renders");
    let response =
        exchange(&daemon.address, &framing::render(&contract.framing, &payload).unwrap()).await;
    assert_eq!(
        response.error.expect("a refusal carries an error").code,
        UNSUPPORTED_CONTROL_VERSION_CODE
    );
    mismatched.control_version = contract.control.version;

    let unknown = frame(&contract, "unknown", "daemon.restart", serde_json::json!({}));
    let response = exchange(&daemon.address, &unknown).await;
    assert_eq!(response.error.expect("a refusal carries an error").code, METHOD_NOT_FOUND_CODE);

    ping(&daemon.address, "after-refusals").await;
    finish(daemon).await;
}

#[tokio::test]
async fn an_established_idle_connection_has_no_incomplete_frame_deadline() {
    let contract = FoundationContract::embedded();
    let root = temporary_runtime_root("i");
    let daemon = start_daemon(&root, ENVIRONMENT).await;
    let mut stream = connect(&daemon.address).await;
    stream
        .write_all(&frame(&contract, "first", PING_METHOD, serde_json::json!({})))
        .await
        .expect("the request is written");
    let payload = local_server::read_frame(&mut stream, &contract, true)
        .await
        .expect("the response arrives")
        .expect("the response is a whole frame");
    let first: ControlResponse = serde_json::from_slice(&payload).expect("the response reads");
    assert_eq!(first.outcome, ResponseOutcome::Success);

    tokio::task::yield_now().await;
    stream
        .write_all(&frame(&contract, "second", PING_METHOD, serde_json::json!({})))
        .await
        .expect("the second request is written");
    let payload = local_server::read_frame(&mut stream, &contract, true)
        .await
        .expect("the second response arrives")
        .expect("the second response is a whole frame");
    let second: ControlResponse = serde_json::from_slice(&payload).expect("the response reads");
    assert_eq!(second.request_identifier, "second");
    finish(daemon).await;
}

#[tokio::test(start_paused = true)]
async fn every_incomplete_peer_closes_at_its_declared_deadline_and_releases_capacity() {
    let contract = FoundationContract::embedded();
    let root = temporary_runtime_root("d");
    let daemon = start_daemon(&root, ENVIRONMENT).await;
    let capacity = contract.server.connection_capacity as usize;

    let mut stalled = Vec::new();
    for index in 0..capacity {
        let mut stream = connect(&daemon.address).await;
        match index % STALLED_PEER_KINDS {
            1 => stream.write_all(&[0_u8, 0]).await.expect("a partial prefix is written"),
            PARTIAL_PAYLOAD_PEER => stream
                .write_all(&[0_u8, 0, 0, 8, b'{'])
                .await
                .expect("a partial payload is written"),
            _ => {}
        }
        stalled.push(stream);
    }

    for mut stream in stalled {
        let mut discarded = [0_u8; 1];
        let read = stream.read(&mut discarded).await.expect("the peer observes the close");
        assert_eq!(read, 0, "the server closed the connection at its declared deadline");
    }

    let recovered = ping(&daemon.address, "after-stalled").await;
    assert_eq!(recovered.readiness_nonce, daemon.service.ownership().readiness_nonce());
    finish(daemon).await;
}

#[tokio::test]
async fn a_second_daemon_for_one_target_does_not_bind_while_another_target_does() {
    let contract = FoundationContract::embedded();
    let root = temporary_runtime_root("t");
    let daemon = start_daemon(&root, ENVIRONMENT).await;
    let namespace =
        RuntimeNamespace::name(&contract, &root, PROFILE, ENVIRONMENT).expect("it names");
    match DaemonOwnership::acquire(&contract, namespace).expect("the runtime state is readable") {
        Acquisition::AlreadyOwned(evidence) => {
            assert_eq!(evidence.namespace_display, format!("{PROFILE}/{ENVIRONMENT}"));
        }
        other => panic!("a second daemon must not bind, but reported {other:?}"),
    }
    let other = start_daemon(&root, "publish").await;
    assert_ne!(other.digest, daemon.digest, "another target is another namespace");
    assert_eq!(ping(&other.address, "other").await.environment, "publish");
    assert_eq!(ping(&daemon.address, "first").await.environment, ENVIRONMENT);
    finish(other).await;
    finish(daemon).await;
}

#[tokio::test]
async fn the_live_nonce_stops_the_daemon_and_a_stale_nonce_cannot_stop_a_replacement() {
    let contract = FoundationContract::embedded();
    let root = temporary_runtime_root("s");
    let daemon = start_daemon(&root, ENVIRONMENT).await;
    let live = ping(&daemon.address, "before-stop").await.readiness_nonce;
    let stale = stale_nonce(&live);

    let refused = exchange(
        &daemon.address,
        &frame(&contract, "stale", STOP_METHOD, serde_json::json!({ "readiness_nonce": stale })),
    )
    .await;
    assert_eq!(refused.error.expect("a refusal carries an error").code, STALE_DAEMON_INSTANCE_CODE);
    ping(&daemon.address, "after-stale-stop").await;

    let acknowledged = exchange(
        &daemon.address,
        &frame(
            &contract,
            "stop",
            STOP_METHOD,
            serde_json::json!({ "readiness_nonce": live.clone() }),
        ),
    )
    .await;
    assert_eq!(acknowledged.outcome, ResponseOutcome::Success);
    daemon.served.await.expect("the accept loop finishes after an authorized stop");

    let lock_path = OwnerLock::path_for(&root, &daemon.digest);
    drop(daemon.service);
    assert!(lock_path.is_file(), "orderly shutdown keeps the persistent lock file");
    assert_eq!(
        readiness::read(&root, &daemon.digest).expect("the record is readable"),
        None,
        "orderly shutdown removes this owner's readiness record"
    );

    let replacement = start_daemon(&root, ENVIRONMENT).await;
    assert_ne!(ping(&replacement.address, "replacement").await.readiness_nonce, live);
    let refused = exchange(
        &replacement.address,
        &frame(&contract, "stale", STOP_METHOD, serde_json::json!({ "readiness_nonce": live })),
    )
    .await;
    assert_eq!(
        refused.error.clone().map(|error| error.code).as_deref(),
        Some(STALE_DAEMON_INSTANCE_CODE),
        "{refused:?}"
    );
    ping(&replacement.address, "still-serving").await;
    finish(replacement).await;
    std::fs::remove_dir_all(&root).ok();
}
