//! The client side of the local daemon connection.
//!
//! A client frames one request, writes it, and reads exactly one response
//! frame back. Nothing here starts a daemon: reaching an endpoint that no
//! process is listening on is reported as absence, so an existing-owner probe
//! can be told apart from a transport failure.

use slingshot_daemon::local_server::{self, ConnectionFailure};
use slingshot_daemon::platform_runtime::endpoint::EndpointAddress;
use slingshot_local_protocol::envelope::{ControlRequest, ControlResponse, ResponseOutcome};
use slingshot_local_protocol::foundation_contract::FoundationContract;
use slingshot_local_protocol::framing;
use slingshot_local_protocol::ping::{PING_METHOD, PingResult};

/// Reason a request could not be exchanged with a daemon.
#[derive(Debug, thiserror::Error)]
pub enum ExchangeFailure {
    /// No process is listening on the endpoint.
    #[error("no daemon is listening on {0}")]
    Absent(String),
    /// The connection could not be used.
    #[error("the connection could not be used: {0}")]
    Transport(String),
    /// The response was not a readable control response.
    #[error("the response was not readable: {0}")]
    Unreadable(String),
    /// The daemon refused the request.
    #[error("the daemon refused the request: {code}: {message}")]
    Refused {
        /// Stable code the daemon reported.
        code: String,
        /// Bounded description the daemon reported.
        message: String,
    },
}

/// Reports whether an operating-system failure means nothing is listening.
fn means_absent(failure: &std::io::Error) -> bool {
    matches!(
        failure.kind(),
        std::io::ErrorKind::NotFound
            | std::io::ErrorKind::ConnectionRefused
            | std::io::ErrorKind::ConnectionReset
    )
}

/// Opens one connection to a daemon endpoint.
#[cfg(unix)]
async fn open(address: &EndpointAddress) -> Result<tokio::net::UnixStream, ExchangeFailure> {
    let EndpointAddress::UnixDomainSocket(path) = address;
    tokio::net::UnixStream::connect(path).await.map_err(|failure| {
        if means_absent(&failure) {
            ExchangeFailure::Absent(address.display())
        } else {
            ExchangeFailure::Transport(failure.to_string())
        }
    })
}

/// Opens one connection to a daemon endpoint.
#[cfg(windows)]
async fn open(
    address: &EndpointAddress,
) -> Result<tokio::net::windows::named_pipe::NamedPipeClient, ExchangeFailure> {
    let EndpointAddress::WindowsNamedPipe(name) = address;
    tokio::net::windows::named_pipe::ClientOptions::new().open(name).map_err(|failure| {
        if means_absent(&failure) {
            ExchangeFailure::Absent(address.display())
        } else {
            ExchangeFailure::Transport(failure.to_string())
        }
    })
}

/// Exchanges one request with the daemon that owns an endpoint.
///
/// # Errors
///
/// Returns [`ExchangeFailure::Absent`] when no process is listening, and the
/// other variants when the connection, the response, or the daemon refuses.
pub async fn exchange(
    contract: &FoundationContract,
    address: &EndpointAddress,
    request: &ControlRequest,
) -> Result<ControlResponse, ExchangeFailure> {
    let mut stream = open(address).await?;
    let payload = serde_json::to_vec(request)
        .map_err(|failure| ExchangeFailure::Unreadable(failure.to_string()))?;
    let frame = framing::render(&contract.framing, &payload)
        .map_err(|failure| ExchangeFailure::Unreadable(failure.to_string()))?;
    local_server::write_frame(&mut stream, contract, &frame)
        .await
        .map_err(|failure| ExchangeFailure::Transport(failure.to_string()))?;
    let response = local_server::read_frame(&mut stream, contract, true)
        .await
        .map_err(|failure: ConnectionFailure| ExchangeFailure::Transport(failure.to_string()))?
        .ok_or_else(|| ExchangeFailure::Absent(address.display()))?;
    serde_json::from_slice(&response)
        .map_err(|failure| ExchangeFailure::Unreadable(failure.to_string()))
}

/// Asks the daemon that owns an endpoint who it is.
///
/// # Errors
///
/// Returns [`ExchangeFailure`] when no daemon is listening, the exchange fails,
/// or the daemon refuses.
pub async fn ping(
    contract: &FoundationContract,
    address: &EndpointAddress,
    request_identifier: &str,
) -> Result<PingResult, ExchangeFailure> {
    let request = ControlRequest {
        control_version: contract.control.version,
        request_identifier: request_identifier.to_owned(),
        method: PING_METHOD.to_owned(),
        arguments: serde_json::json!({}),
    };
    let response = exchange(contract, address, &request).await?;
    match (response.outcome, response.result, response.error) {
        (ResponseOutcome::Success, Some(result), _) => serde_json::from_value(result)
            .map_err(|failure| ExchangeFailure::Unreadable(failure.to_string())),
        (_, _, Some(error)) => {
            Err(ExchangeFailure::Refused { code: error.code, message: error.message })
        }
        _ => Err(ExchangeFailure::Unreadable(
            "the response carried neither result nor error".to_owned(),
        )),
    }
}
