//! Fixed-size, payload-free process counters for transport investigations.
//!
//! Snapshots are cumulative and independently atomic, not a transaction. Compare
//! them at quiescent workflow boundaries in a dedicated process. They contain no
//! origin, credential, request identifier, header, body or individual history.

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
use tokio::time::Instant;

const PHASE_COUNT: usize = 6;

/// Credential domains are counted independently even when hosts coincide.
#[derive(Clone, Copy)]
pub enum Route {
    /// The frozen selected author.
    Author,
    /// The fixed identity-management endpoint.
    IdentityManagement,
}

/// Phases can overlap (HTTP/2 reads while writing); do not sum their durations.
#[derive(Clone, Copy)]
pub enum Phase {
    /// DNS resolution and TCP establishment together.
    Connect,
    /// TLS authentication and negotiation.
    Handshake,
    /// Writing a request, including flow-control waits.
    Request,
    /// Receiving the response head, including informational heads.
    ResponseHead,
    /// Receiving the body under the codec's existing completion rules.
    ResponseBody,
    /// A complete HTTP exchange, excluding connection establishment.
    Exchange,
}

/// Aggregate outcomes and monotonic elapsed nanoseconds for one phase.
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct PhaseSnapshot {
    /// Futures actually polled, including those still in flight.
    pub started: u64,
    /// Futures that returned success.
    pub succeeded: u64,
    /// Futures that returned failure, including phase deadlines.
    pub failed: u64,
    /// Futures dropped before returning, including outer deadline cancellation.
    pub abandoned: u64,
    /// Sum over finished or abandoned phases; excludes in-flight time.
    pub elapsed_nanoseconds: u64,
}

/// Fixed-size cumulative observations for one credential domain.
#[derive(Debug, serde::Serialize)]
pub struct Snapshot {
    /// Named phase counters, in declaration order.
    pub phases: [PhaseSnapshot; PHASE_COUNT],
    /// Names corresponding to `phases`.
    pub phase_names: [&'static str; PHASE_COUNT],
    /// Bytes read from sockets, including TLS records where applicable.
    pub received_bytes: u64,
    /// Bytes accepted by socket writes, including TLS records.
    pub transmitted_bytes: u64,
    /// Successfully connected sockets handed to the observation wrapper.
    pub opened_sockets: u64,
    /// Wrapped sockets whose owner dropped them, regardless of shutdown state.
    pub dropped_sockets: u64,
}

#[derive(Default)]
struct PhaseCounters {
    started: AtomicU64,
    succeeded: AtomicU64,
    failed: AtomicU64,
    abandoned: AtomicU64,
    elapsed_nanoseconds: AtomicU64,
}

#[derive(Default)]
struct Counters {
    phases: [PhaseCounters; PHASE_COUNT],
    received: AtomicU64,
    transmitted: AtomicU64,
    opened: AtomicU64,
    dropped: AtomicU64,
}

static AUTHOR: std::sync::LazyLock<Counters> = std::sync::LazyLock::new(Counters::default);
static IDENTITY_MANAGEMENT: std::sync::LazyLock<Counters> =
    std::sync::LazyLock::new(Counters::default);

impl Route {
    fn counters(self) -> &'static Counters {
        match self {
            Self::Author => &AUTHOR,
            Self::IdentityManagement => &IDENTITY_MANAGEMENT,
        }
    }

    /// Reads cumulative counters. Concurrent changes can span this snapshot.
    #[must_use]
    pub fn snapshot(self) -> Snapshot {
        let counters = self.counters();
        Snapshot {
            phases: std::array::from_fn(|index| counters.phases[index].snapshot()),
            phase_names: [
                "connect",
                "handshake",
                "request",
                "response_head",
                "response_body",
                "exchange",
            ],
            received_bytes: counters.received.load(Ordering::Relaxed),
            transmitted_bytes: counters.transmitted.load(Ordering::Relaxed),
            opened_sockets: counters.opened.load(Ordering::Relaxed),
            dropped_sockets: counters.dropped.load(Ordering::Relaxed),
        }
    }
}

impl PhaseCounters {
    fn snapshot(&self) -> PhaseSnapshot {
        PhaseSnapshot {
            started: self.started.load(Ordering::Relaxed),
            succeeded: self.succeeded.load(Ordering::Relaxed),
            failed: self.failed.load(Ordering::Relaxed),
            abandoned: self.abandoned.load(Ordering::Relaxed),
            elapsed_nanoseconds: self.elapsed_nanoseconds.load(Ordering::Relaxed),
        }
    }
}

fn add(counter: &AtomicU64, amount: u64) {
    if amount == 0 {
        return;
    }
    let _previous = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |held| {
        Some(held.saturating_add(amount))
    });
}

struct Observation<'held> {
    counters: &'held PhaseCounters,
    started: Instant,
    outcome: &'held AtomicU64,
}

impl<'held> Observation<'held> {
    fn new(counters: &'held PhaseCounters) -> Self {
        add(&counters.started, 1);
        Self { counters, started: Instant::now(), outcome: &counters.abandoned }
    }
}

impl Drop for Observation<'_> {
    fn drop(&mut self) {
        add(self.outcome, 1);
        add(
            &self.counters.elapsed_nanoseconds,
            u64::try_from(self.started.elapsed().as_nanos()).unwrap_or(u64::MAX),
        );
    }
}

/// Observes a result without retrying, spawning, changing or retaining its value.
///
/// # Errors
/// Returns the operation's error unchanged. Dropping the future drops its work.
pub fn observe<Value, Failure>(
    route: Route,
    phase: Phase,
    operation: impl Future<Output = Result<Value, Failure>>,
) -> impl Future<Output = Result<Value, Failure>> {
    // Large codec futures must not be copied through another inline state machine.
    // The owned box is bounded by the existing concurrent exchange limit.
    Box::pin(async move { observe_with(&route.counters().phases[phase as usize], operation).await })
}

async fn observe_with<Value, Failure>(
    counters: &PhaseCounters,
    operation: impl Future<Output = Result<Value, Failure>>,
) -> Result<Value, Failure> {
    let mut observation = Observation::new(counters);
    let result = operation.await;
    observation.outcome = if result.is_ok() { &counters.succeeded } else { &counters.failed };
    result
}

/// A socket wrapper below TLS: counts bytes only, and owns no background task.
pub struct ObservedSocket {
    socket: TcpStream,
    counters: &'static Counters,
}

impl ObservedSocket {
    /// Records one established socket in its credential domain.
    pub fn new(socket: TcpStream, route: Route) -> Self {
        let counters = route.counters();
        add(&counters.opened, 1);
        Self { socket, counters }
    }
}

impl ::core::fmt::Debug for ObservedSocket {
    fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        formatter.write_str("ObservedSocket([redacted])")
    }
}

impl Drop for ObservedSocket {
    fn drop(&mut self) {
        add(&self.counters.dropped, 1);
    }
}

impl AsyncRead for ObservedSocket {
    fn poll_read(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let before = buffer.filled().len();
        let result = Pin::new(&mut self.socket).poll_read(context, buffer);
        add(
            &self.counters.received,
            u64::try_from(buffer.filled().len() - before).unwrap_or(u64::MAX),
        );
        result
    }
}

impl AsyncWrite for ObservedSocket {
    fn poll_write(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let result = Pin::new(&mut self.socket).poll_write(context, bytes);
        if let Poll::Ready(Ok(count)) = &result {
            add(&self.counters.transmitted, u64::try_from(*count).unwrap_or(u64::MAX));
        }
        result
    }

    fn poll_flush(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.socket).poll_flush(context)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.socket).poll_shutdown(context)
    }
}

/// Tracks the head-to-body transition of a streaming response without history.
pub(crate) struct ResponseObservation {
    counters: &'static Counters,
    current: Observation<'static>,
    body_started: bool,
}

impl ResponseObservation {
    /// Marks the first validated final head; later calls have no effect.
    pub(crate) fn head_complete(&mut self) {
        if self.body_started {
            return;
        }
        self.body_started = true;
        self.current.outcome = &self.current.counters.succeeded;
        self.current = Observation::new(&self.counters.phases[Phase::ResponseBody as usize]);
    }
}

/// Measures one response, ending its current phase with the original outcome.
///
/// # Errors
/// Returns the reader's error unchanged; outer cancellation remains abandonment.
pub(crate) fn observe_response<Value, Failure>(
    route: Route,
    operation: impl AsyncFnOnce(&mut ResponseObservation) -> Result<Value, Failure>,
) -> impl Future<Output = Result<Value, Failure>> {
    Box::pin(async move { observe_response_with(route.counters(), operation).await })
}

async fn observe_response_with<Value, Failure>(
    counters: &'static Counters,
    operation: impl AsyncFnOnce(&mut ResponseObservation) -> Result<Value, Failure>,
) -> Result<Value, Failure> {
    let mut observation = ResponseObservation {
        counters,
        current: Observation::new(&counters.phases[Phase::ResponseHead as usize]),
        body_started: false,
    };
    let result = operation(&mut observation).await;
    observation.current.outcome = if result.is_ok() {
        &observation.current.counters.succeeded
    } else {
        &observation.current.counters.failed
    };
    result
}

#[cfg(test)]
mod tests;
