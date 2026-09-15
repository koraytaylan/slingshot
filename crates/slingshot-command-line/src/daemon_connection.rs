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
use slingshot_local_protocol::message::{OperationEnvelope, OperationResponse};
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

/// Exchanges one versioned operation with the daemon that owns an endpoint.
///
/// The same framing as retained control, and a different payload: a versioned
/// envelope carries the target, the revision, and the contract this client was
/// built against, so the daemon can refuse a caller it does not agree with
/// before it acts rather than after.
///
/// # Errors
///
/// Returns [`ExchangeFailure::Absent`] when no process is listening, and the
/// other variants when the connection or the answer cannot be used.
pub async fn exchange_operation(
    contract: &FoundationContract,
    address: &EndpointAddress,
    envelope: &OperationEnvelope,
) -> Result<OperationResponse, ExchangeFailure> {
    let mut stream = open(address).await?;
    let payload = serde_json::to_vec(envelope)
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

/// What one artifact transfer says as it arrives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactEvent {
    /// The transfer is beginning, and this is what it will be.
    Start {
        /// Which artifact.
        artifact_identifier: String,
        /// How long it is.
        byte_length: u64,
        /// What it digests to.
        content_digest: String,
        /// What kind of bytes it holds.
        media_type: String,
    },
    /// These bytes are part of it, in order.
    Chunk(Vec<u8>),
}

/// One artifact's bytes, read from the daemon that owns the endpoint.
///
/// An artifact read is answered with many frames on one connection rather than
/// one: `ArtifactStart` says what is coming, zero or more `ArtifactChunk`
/// frames carry it, and `ArtifactEnd` says it ended. Reading only the first
/// would leave the chunks unread on a connection that is then closed, which is
/// why this is its own exchange rather than a variant of
/// [`exchange_operation`].
///
/// Every event is handed to `observe` as it arrives, so a caller bounds and
/// verifies the bytes while the transfer is happening rather than after the
/// daemon has sent everything.
///
/// # Errors
///
/// Returns [`ArtifactStreamRefusal`] when the exchange fails, when the daemon
/// ends the transfer before it said it would, or when it answers a frame an
/// artifact transfer may not carry. A frame that is not a transfer at all is
/// returned to the caller, which is the daemon's answer to a request that was
/// not an artifact read.
pub async fn stream_artifact<Observe>(
    contract: &FoundationContract,
    address: &EndpointAddress,
    envelope: &OperationEnvelope,
    mut observe: Observe,
) -> Result<OperationResponse, ArtifactStreamRefusal>
where
    Observe: FnMut(ArtifactEvent) -> Result<(), String>,
{
    let mut stream = open(address).await.map_err(ArtifactStreamRefusal::Exchange)?;
    let payload = serde_json::to_vec(envelope).map_err(|failure| {
        ArtifactStreamRefusal::Exchange(ExchangeFailure::Unreadable(failure.to_string()))
    })?;
    let frame = framing::render(&contract.framing, &payload).map_err(|failure| {
        ArtifactStreamRefusal::Exchange(ExchangeFailure::Unreadable(failure.to_string()))
    })?;
    local_server::write_frame(&mut stream, contract, &frame).await.map_err(|failure| {
        ArtifactStreamRefusal::Exchange(ExchangeFailure::Transport(failure.to_string()))
    })?;
    let mut reader = local_server::FrameReader::new();
    let Some(first) = read_next(&mut reader, &mut stream, contract).await? else {
        return Err(ArtifactStreamRefusal::Exchange(ExchangeFailure::Absent(address.display())));
    };
    let start = serde_json::from_slice::<OperationResponse>(&first).map_err(|failure| {
        ArtifactStreamRefusal::Exchange(ExchangeFailure::Unreadable(failure.to_string()))
    })?;
    let OperationResponse::ArtifactStart {
        artifact_identifier,
        byte_length,
        content_digest,
        media_type,
    } = &start
    else {
        // Anything else is the daemon's answer to a request that was not an
        // artifact read, and this exchange has nothing to stream.
        return Ok(start);
    };
    observe(ArtifactEvent::Start {
        artifact_identifier: artifact_identifier.clone(),
        byte_length: *byte_length,
        content_digest: content_digest.clone(),
        media_type: media_type.clone(),
    })
    .map_err(ArtifactStreamRefusal::AbsorbRefused)?;
    loop {
        let Some(frame) = read_next(&mut reader, &mut stream, contract).await? else {
            return Err(ArtifactStreamRefusal::EndedEarly);
        };
        let response = serde_json::from_slice::<OperationResponse>(&frame).map_err(|failure| {
            ArtifactStreamRefusal::Exchange(ExchangeFailure::Unreadable(failure.to_string()))
        })?;
        match response {
            OperationResponse::ArtifactChunk { body } => {
                use base64::Engine as _;
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(&body.encoded_bytes)
                    .map_err(|_| ArtifactStreamRefusal::ChunkUnreadable)?;
                observe(ArtifactEvent::Chunk(bytes))
                    .map_err(ArtifactStreamRefusal::AbsorbRefused)?;
            }
            OperationResponse::ArtifactEnd => return Ok(start),
            other => return Err(ArtifactStreamRefusal::OtherFrame(Box::new(other))),
        }
    }
}

/// Reads one more frame, or nothing when the connection ended cleanly.
async fn read_next(
    reader: &mut local_server::FrameReader,
    stream: &mut tokio::net::UnixStream,
    contract: &FoundationContract,
) -> Result<Option<Vec<u8>>, ArtifactStreamRefusal> {
    reader.read(stream, contract, false).await.map_err(|failure| {
        ArtifactStreamRefusal::Exchange(ExchangeFailure::Transport(failure.to_string()))
    })
}

/// Why one artifact stream could not be read.
#[derive(Debug, thiserror::Error)]
pub enum ArtifactStreamRefusal {
    /// The exchange itself failed.
    #[error(transparent)]
    Exchange(#[from] ExchangeFailure),
    /// A chunk was not the base64 the protocol carries bytes in.
    #[error("the daemon sent a chunk this build cannot read")]
    ChunkUnreadable,
    /// The connection ended before the transfer did.
    #[error("the daemon ended the transfer before it said it would")]
    EndedEarly,
    /// The caller would not take what arrived.
    #[error("the artifact could not be taken: {0}")]
    AbsorbRefused(String),
    /// A frame arrived that an artifact transfer may not carry.
    #[error("the daemon answered a frame an artifact transfer does not carry")]
    OtherFrame(Box<OperationResponse>),
}

/// What a client expects the daemon owning its target to be serving.
///
/// Compared against what the owner says about itself before a single request is
/// sent, because the first request is already too late: a client that asked a
/// daemon serving another target to do something would have asked the wrong
/// remote system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectedTarget {
    /// The opaque author-target identity digest.
    pub author_target_identity_digest: String,
    /// The operation protocol version this client speaks.
    pub operation_protocol_version: u64,
    /// The environment revision this client selected.
    pub selected_environment_revision: String,
}

/// What an owner said about itself when it answered hello.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedOwner {
    /// The opaque author-target identity digest it serves.
    pub author_target_identity_digest: String,
    /// The namespace it owns, for guidance a person can act on.
    pub namespace_display: String,
    /// Its live readiness nonce.
    pub readiness_nonce: String,
    /// The environment revision it started from.
    pub selected_environment_revision: String,
    /// The operation protocol versions it can serve.
    pub supported_operation_versions: Vec<u64>,
}

/// What a client should do about the daemon that owns its target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OwnerDisposition {
    /// Nobody owns the target, so this client may contend to start one.
    Absent,
    /// The owner serves this target and speaks this client's version.
    Matching {
        /// Its live readiness nonce.
        readiness_nonce: String,
    },
    /// The owner serves this target but not this client's operation version.
    ///
    /// Not a mismatch of identity, so retained control still works and the
    /// owner is still the right one - it just cannot run this client's
    /// operations.
    OperationIncompatible {
        /// What it can serve.
        supported_operation_versions: Vec<u64>,
    },
    /// The owner serves something else, and this client must not disturb it.
    Mismatched {
        /// What a person should do, in words they can act on.
        guidance: String,
        /// Which of the two differs, for a caller that wants to say.
        reason: MismatchReason,
    },
}

/// Which part of an owner's identity differs from what a client expected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MismatchReason {
    /// It serves a different author target.
    Target,
    /// It serves the same target at a different environment revision.
    Revision,
}

/// Returns what a client should do about `owner`.
///
/// A mismatch is never joined, routed to, stopped, or signalled. The owner is
/// somebody else's daemon serving somebody else's target, and the only correct
/// action is to tell a person which namespace to look at and let them decide.
/// Automatically stopping it would end work this client knows nothing about.
#[must_use]
pub fn classify_owner(
    expected: &ExpectedTarget,
    owner: Option<&ObservedOwner>,
) -> OwnerDisposition {
    let Some(owner) = owner else {
        return OwnerDisposition::Absent;
    };
    if owner.author_target_identity_digest != expected.author_target_identity_digest {
        return mismatched(expected, owner, MismatchReason::Target);
    }
    if owner.selected_environment_revision != expected.selected_environment_revision {
        return mismatched(expected, owner, MismatchReason::Revision);
    }
    if !owner.supported_operation_versions.contains(&expected.operation_protocol_version) {
        return OwnerDisposition::OperationIncompatible {
            supported_operation_versions: owner.supported_operation_versions.clone(),
        };
    }
    OwnerDisposition::Matching { readiness_nonce: owner.readiness_nonce.clone() }
}

/// Returns the refusal one mismatched owner produces.
fn mismatched(
    expected: &ExpectedTarget,
    owner: &ObservedOwner,
    reason: MismatchReason,
) -> OwnerDisposition {
    let differs = match reason {
        MismatchReason::Target => "another author target",
        MismatchReason::Revision => "another environment revision",
    };
    OwnerDisposition::Mismatched {
        guidance: format!(
            "the daemon owning {} serves {differs}; inspect it with \
             `slingshot daemon status --profile <profile> --environment <environment>` and stop \
             it explicitly if that is what you want. This client will not stop it for you, \
             because it may be running work you cannot see. Expected target {}, revision {}.",
            owner.namespace_display,
            expected.author_target_identity_digest,
            expected.selected_environment_revision
        ),
        reason,
    }
}
