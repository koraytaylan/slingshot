//! The local accept loop of one owned runtime namespace.
//!
//! The server binds only while its ownership is held, serves at most the
//! connection capacity the foundation contract declares, and applies exactly
//! the contract's deadlines: one for the first control frame of a connection,
//! one between two reads while a frame is incomplete, one for completing a
//! frame however slowly it arrives, and one for writing a response. A
//! connection that has finished a frame and gone quiet has no incomplete-frame
//! deadline, because nothing is incomplete.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use slingshot_local_protocol::foundation_contract::FoundationContract;
use slingshot_local_protocol::framing::{self, FrameProgress, FramingFailure};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::Semaphore;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use crate::platform_runtime::endpoint::EndpointAddress;
use crate::platform_runtime::failure::PlatformFailure;
use crate::service::DaemonService;

/// Bytes one read may add to a partly received frame.
const CONNECTION_READ_CHUNK_BYTES: usize = 8_192;

/// Reason one connection ended before it was served.
#[derive(Debug, thiserror::Error)]
pub enum ConnectionFailure {
    /// The peer closed while a frame was incomplete.
    #[error("the peer closed while a frame was incomplete")]
    Truncated,
    /// A deadline the contract declares elapsed.
    #[error("the {stage} deadline of {deadline:?} elapsed")]
    DeadlineElapsed {
        /// Which deadline elapsed.
        stage: &'static str,
        /// The deadline that elapsed.
        deadline: Duration,
    },
    /// The frame broke a bound the contract declares.
    #[error("the frame broke a declared bound: {0}")]
    Framing(#[from] FramingFailure),
    /// The connection could not be read or written.
    #[error("the connection could not be used: {0}")]
    Transport(String),
}

/// Reason the local server could not bind or serve.
#[derive(Debug, thiserror::Error)]
pub enum ServerFailure {
    /// A readiness stage was reached before the one that precedes it.
    #[error("this daemon reached {reached} while {expected} was still outstanding")]
    StageOutOfOrder {
        /// The stage that was still outstanding.
        expected: String,
        /// The stage that was reached anyway.
        reached: String,
    },
    /// The connection capacity the contract declares is already in use.
    #[error("this daemon serves {capacity} connections at once, and all of them are in use")]
    CapacityInUse {
        /// How many it serves at once.
        capacity: u32,
    },
    /// The endpoint could not be bound.
    #[error("the endpoint {address} could not be bound: {reason}")]
    Unbindable {
        /// Display value of the endpoint.
        address: String,
        /// Operating-system reason the endpoint could not be bound.
        reason: String,
    },
    /// A platform operation failed.
    #[error(transparent)]
    Platform(#[from] PlatformFailure),
}

/// The listener of one runtime namespace.
#[derive(Debug)]
pub struct LocalListener {
    #[cfg(unix)]
    inner: tokio::net::UnixListener,
    #[cfg(unix)]
    path: PathBuf,
    #[cfg(windows)]
    name: String,
    #[cfg(windows)]
    pending: Option<tokio::net::windows::named_pipe::NamedPipeServer>,
}

impl LocalListener {
    /// Binds the endpoint of one owned runtime namespace.
    ///
    /// A stale endpoint object is removed first. That is safe only because the
    /// caller already holds the owner lock, which proves the prior owner is
    /// gone.
    ///
    /// # Errors
    ///
    /// Returns [`ServerFailure::Unbindable`] when the endpoint cannot be bound.
    #[cfg(unix)]
    pub fn bind(address: &EndpointAddress) -> Result<Self, ServerFailure> {
        let EndpointAddress::UnixDomainSocket(path) = address;
        let unbindable =
            |reason: String| ServerFailure::Unbindable { address: address.display(), reason };
        if path.exists() {
            std::fs::remove_file(path).map_err(|failure| unbindable(failure.to_string()))?;
        }
        let inner = tokio::net::UnixListener::bind(path)
            .map_err(|failure| unbindable(failure.to_string()))?;
        Ok(Self { inner, path: path.clone() })
    }

    /// Binds the endpoint of one owned runtime namespace.
    ///
    /// Every named-pipe server instance is created with the remote-client
    /// rejection the supported-target row requires.
    ///
    /// # Errors
    ///
    /// Returns [`ServerFailure::Unbindable`] when the endpoint cannot be bound.
    #[cfg(windows)]
    pub fn bind(address: &EndpointAddress) -> Result<Self, ServerFailure> {
        let EndpointAddress::WindowsNamedPipe(name) = address;
        let pending = create_pipe_instance(name, true)?;
        Ok(Self { name: name.clone(), pending: Some(pending) })
    }

    /// Accepts one connection.
    ///
    /// # Errors
    ///
    /// Returns [`ServerFailure::Unbindable`] when the endpoint cannot accept.
    #[cfg(unix)]
    pub async fn accept(&mut self) -> Result<tokio::net::UnixStream, ServerFailure> {
        let (stream, _) =
            self.inner.accept().await.map_err(|failure| ServerFailure::Unbindable {
                address: self.path.display().to_string(),
                reason: failure.to_string(),
            })?;
        Ok(stream)
    }

    /// Accepts one connection.
    ///
    /// # Errors
    ///
    /// Returns [`ServerFailure::Unbindable`] when the endpoint cannot accept.
    #[cfg(windows)]
    pub async fn accept(
        &mut self,
    ) -> Result<tokio::net::windows::named_pipe::NamedPipeServer, ServerFailure> {
        let server = match self.pending.take() {
            Some(server) => server,
            None => create_pipe_instance(&self.name, false)?,
        };
        server.connect().await.map_err(|failure| ServerFailure::Unbindable {
            address: self.name.clone(),
            reason: failure.to_string(),
        })?;
        self.pending = Some(create_pipe_instance(&self.name, false)?);
        Ok(server)
    }

    /// Removes the endpoint object this listener created.
    #[cfg(unix)]
    pub fn remove(&self) {
        std::fs::remove_file(&self.path).ok();
    }

    /// Removes the endpoint object this listener created.
    ///
    /// A named pipe has no filesystem object: its lifetime follows the live
    /// server handles, so there is nothing to unlink.
    #[cfg(windows)]
    pub fn remove(&self) {}
}

/// The order a daemon must reach readiness in.
///
/// Written down as a value rather than left implicit in the order some function
/// happens to call things, because the ordering is the guarantee. A client that
/// can see readiness is entitled to assume every earlier stage held, and an
/// implementation that quietly bound its endpoint one stage too early would
/// still pass every test that only checked the stages individually.
pub const READINESS_STAGES: &[&str] = &[
    "ownership",
    "selected environment snapshot",
    "installation comparison",
    "database migration",
    "cross-partition audit",
    "listener bound",
    "hello answerable",
    "readiness published",
];

/// How far towards readiness one daemon has got.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ReadinessProgress {
    /// How many stages have completed, as an index into [`READINESS_STAGES`].
    reached: usize,
}

impl ReadinessProgress {
    /// Returns a daemon that has completed nothing.
    #[must_use]
    pub fn started() -> Self {
        Self { reached: 0 }
    }

    /// Returns the next stage this daemon has to complete.
    #[must_use]
    pub fn next_stage(self) -> Option<&'static str> {
        READINESS_STAGES.get(self.reached).copied()
    }

    /// Completes `stage`, or refuses because it is not the one that is next.
    ///
    /// # Errors
    ///
    /// Returns [`ServerFailure::StageOutOfOrder`] naming what was expected. A
    /// stage skipped is a guarantee skipped, so this refuses rather than
    /// tolerating the order and hoping.
    pub fn complete(self, stage: &str) -> Result<Self, ServerFailure> {
        match self.next_stage() {
            Some(expected) if expected == stage => Ok(Self { reached: self.reached + 1 }),
            expected => Err(ServerFailure::StageOutOfOrder {
                expected: expected.unwrap_or("nothing further").to_owned(),
                reached: stage.to_owned(),
            }),
        }
    }

    /// Returns whether the listener may bind yet.
    #[must_use]
    pub fn may_bind(self) -> bool {
        self.reached >= binding_stage()
    }

    /// Returns whether readiness may be published yet.
    ///
    /// Everything before publication has to be done, including answering
    /// hello: a readiness record naming an endpoint that cannot yet answer is
    /// a record that lies for as long as the gap lasts.
    #[must_use]
    pub fn may_publish(self) -> bool {
        self.reached + 1 >= READINESS_STAGES.len()
    }

    /// Returns whether this daemon is ready.
    #[must_use]
    pub fn is_ready(self) -> bool {
        self.reached == READINESS_STAGES.len()
    }
}

/// Returns the index of the stage at which a listener may bind.
fn binding_stage() -> usize {
    READINESS_STAGES
        .iter()
        .position(|stage| *stage == "listener bound")
        .unwrap_or(READINESS_STAGES.len())
}

/// Creates one named-pipe server instance with remote clients rejected.
#[cfg(windows)]
fn create_pipe_instance(
    name: &str,
    first: bool,
) -> Result<tokio::net::windows::named_pipe::NamedPipeServer, ServerFailure> {
    tokio::net::windows::named_pipe::ServerOptions::new()
        .first_pipe_instance(first)
        .reject_remote_clients(true)
        .create(name)
        .map_err(|failure| ServerFailure::Unbindable {
            address: name.to_owned(),
            reason: failure.to_string(),
        })
}

/// Returns the deadline that applies to the next read of one frame.
fn read_deadline(
    contract: &FoundationContract,
    progress: FrameProgress,
    first_frame: bool,
    frame_started: Option<Instant>,
) -> Option<(&'static str, Duration)> {
    match (progress, frame_started) {
        (FrameProgress::Empty, _) if first_frame => {
            Some(("initial control frame", contract.server.initial_control_frame()))
        }
        (FrameProgress::Empty, _) => None,
        (_, Some(started)) => {
            let elapsed = started.elapsed();
            let absolute = contract.server.absolute_frame_completion().saturating_sub(elapsed);
            let idle = contract.server.incomplete_frame_read_idle();
            if absolute <= idle {
                Some(("absolute frame completion", absolute))
            } else {
                Some(("incomplete frame read idle", idle))
            }
        }
        (_, None) => {
            Some(("incomplete frame read idle", contract.server.incomplete_frame_read_idle()))
        }
    }
}

/// Reads one whole frame, or reports that the peer closed cleanly.
///
/// # Errors
///
/// Returns [`ConnectionFailure`] when a deadline elapses, the peer closes with
/// a frame incomplete, the frame breaks a declared bound, or the connection
/// cannot be read.
pub async fn read_frame<Stream>(
    stream: &mut Stream,
    contract: &FoundationContract,
    first_frame: bool,
) -> Result<Option<Vec<u8>>, ConnectionFailure>
where
    Stream: AsyncRead + Unpin,
{
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; CONNECTION_READ_CHUNK_BYTES];
    let mut frame_started: Option<Instant> = None;
    loop {
        let progress = framing::progress(&contract.framing, &buffer)?;
        if let FrameProgress::Complete { declared } = progress {
            let start = contract.framing.length_prefix_bytes as usize;
            return Ok(Some(buffer[start..start + declared].to_vec()));
        }
        let deadline = read_deadline(contract, progress, first_frame, frame_started);
        let read = match deadline {
            Some((stage, deadline)) => tokio::time::timeout(deadline, stream.read(&mut chunk))
                .await
                .map_err(|_| ConnectionFailure::DeadlineElapsed { stage, deadline })?,
            None => stream.read(&mut chunk).await,
        };
        let read = read.map_err(|failure| ConnectionFailure::Transport(failure.to_string()))?;
        if read == 0 {
            return if buffer.is_empty() { Ok(None) } else { Err(ConnectionFailure::Truncated) };
        }
        if frame_started.is_none() {
            frame_started = Some(Instant::now());
        }
        buffer.extend_from_slice(&chunk[..read]);
    }
}

/// Writes one response frame inside the contract's write deadline.
///
/// # Errors
///
/// Returns [`ConnectionFailure`] when the deadline elapses or the connection
/// cannot be written.
pub async fn write_frame<Stream>(
    stream: &mut Stream,
    contract: &FoundationContract,
    frame: &[u8],
) -> Result<(), ConnectionFailure>
where
    Stream: AsyncWrite + Unpin,
{
    let deadline = contract.server.response_write();
    let written = tokio::time::timeout(deadline, stream.write_all(frame))
        .await
        .map_err(|_| ConnectionFailure::DeadlineElapsed { stage: "response write", deadline })?;
    written.map_err(|failure| ConnectionFailure::Transport(failure.to_string()))?;
    tokio::time::timeout(deadline, stream.flush())
        .await
        .map_err(|_| ConnectionFailure::DeadlineElapsed { stage: "response write", deadline })?
        .map_err(|failure| ConnectionFailure::Transport(failure.to_string()))
}

/// Serves one connection until its peer closes or a stop is authorized.
///
/// # Errors
///
/// Returns [`ConnectionFailure`] when the connection ends before it was served.
pub async fn serve_connection<Stream>(
    service: &DaemonService,
    stream: &mut Stream,
) -> Result<bool, ConnectionFailure>
where
    Stream: AsyncRead + AsyncWrite + Unpin,
{
    let contract = service.contract();
    let mut first_frame = true;
    loop {
        let Some(payload) = read_frame(stream, contract, first_frame).await? else {
            return Ok(false);
        };
        first_frame = false;
        let outcome = service.answer(&payload);
        write_frame(stream, contract, outcome.frame()).await?;
        if outcome.stops() {
            return Ok(true);
        }
    }
}

/// Serves one owned runtime namespace until a stop is authorized.
///
/// At most the contract's connection capacity is served at once, so a cohort of
/// slow peers cannot strand the endpoint: each connection releases its share
/// when it closes or its deadline elapses.
///
/// # Errors
///
/// Returns [`ServerFailure`] when the endpoint cannot accept a connection.
pub async fn serve(
    service: Arc<DaemonService>,
    listener: &mut LocalListener,
    shutdown: CancellationToken,
) -> Result<(), ServerFailure> {
    let capacity = service.contract().server.connection_capacity as usize;
    let permits = Arc::new(Semaphore::new(capacity));
    loop {
        let permit = tokio::select! {
            biased;
            () = shutdown.cancelled() => break,
            permit = Arc::clone(&permits).acquire_owned() => match permit {
                Ok(permit) => permit,
                Err(_) => break,
            },
        };
        let accepted = tokio::select! {
            biased;
            () = shutdown.cancelled() => break,
            accepted = listener.accept() => accepted?,
        };
        let served = Arc::clone(&service);
        let stopper = shutdown.clone();
        tokio::spawn(async move {
            let mut stream = accepted;
            if serve_connection(served.as_ref(), &mut stream).await.unwrap_or(false) {
                stopper.cancel();
            }
            drop(permit);
        });
    }
    Ok(())
}

/// How many connections this daemon is serving at once.
///
/// A permit is held for as long as a connection is, and released when it ends
/// however it ends. Capacity that leaked on an abrupt close would be capacity
/// nobody could get back without restarting the daemon, which is the failure
/// mode a bound is supposed to prevent rather than cause.
#[derive(Debug)]
pub struct ConnectionCapacity {
    /// How many may be served at once.
    capacity: u32,
    /// How many are being served now.
    in_use: std::sync::Arc<std::sync::atomic::AtomicU32>,
}

/// One connection's claim on the capacity, released when it is dropped.
#[derive(Debug)]
pub struct ConnectionPermit {
    /// The counter to give the claim back to.
    in_use: std::sync::Arc<std::sync::atomic::AtomicU32>,
}

impl ConnectionCapacity {
    /// Returns the capacity the foundation contract declares.
    #[must_use]
    pub fn declared(contract: &FoundationContract) -> Self {
        Self {
            capacity: contract.server.connection_capacity,
            in_use: std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
        }
    }

    /// Returns how many connections are being served now.
    #[must_use]
    pub fn in_use(&self) -> u32 {
        self.in_use.load(std::sync::atomic::Ordering::Acquire)
    }

    /// Claims capacity for one connection.
    ///
    /// # Errors
    ///
    /// Returns [`ServerFailure::CapacityInUse`], deterministically: a client
    /// arriving at a full daemon is refused rather than queued, because a queue
    /// with no bound is the same problem one layer along.
    pub fn claim(&self) -> Result<ConnectionPermit, ServerFailure> {
        let claimed = self
            .in_use
            .fetch_update(
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
                |held| (held < self.capacity).then_some(held + 1),
            )
            .is_ok();
        if claimed {
            Ok(ConnectionPermit { in_use: std::sync::Arc::clone(&self.in_use) })
        } else {
            Err(ServerFailure::CapacityInUse { capacity: self.capacity })
        }
    }
}

impl Drop for ConnectionPermit {
    fn drop(&mut self) {
        self.in_use.fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
    }
}
