//! Concurrent request intake with bounded execution and output ownership.
//!
//! The coordinator alone owns session state. Workers get fresh runners and
//! cancellation flags. Output acknowledgement, rather than worker completion,
//! releases successful request identities. EOF drains admitted work within the
//! shutdown budget; a broken or stalled output cancels all local waits.

use std::io::{self, BufRead, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError};
use std::time::{Duration, Instant};

use super::dispatch::{Completion, DeferredRequest};
use super::stream_workers::Streams;
use super::{ServerApplication, identifier_key};
use crate::model_context_protocol::active_request_registry::MAXIMUM_ACTIVE_REQUESTS;
use crate::model_context_protocol::operation_execution::ToolRunner;
use crate::model_context_protocol::standard_stream_transport::{
    BoundedLine, Message, OutputFailure, Written, maximum_line_bytes, maximum_queued_bytes,
    maximum_queued_messages, queue_pressure_deadline, read_message, shutdown_deadline,
    write_deadline,
};

/// Wakeups also poll process signals and monotonic deadlines at this interval.
const COORDINATOR_POLL_INTERVAL: Duration = Duration::from_millis(20);

/// Space reserved for an immediate control answer and the sole output writer.
const CONTROL_AND_WRITER_SLOTS: usize = 2;

/// Returns the worker bound derived from the declared response memory budget.
#[must_use]
pub fn maximum_workers() -> usize {
    maximum_queued_bytes()
        .checked_div(maximum_line_bytes())
        .unwrap_or(0)
        .saturating_sub(CONTROL_AND_WRITER_SLOTS)
        .min(MAXIMUM_ACTIVE_REQUESTS)
}

/// Runs one process session over owned streams and per-request runner instances.
///
/// The streams must belong to this session; the caller must not hold their
/// locks. Stream threads may outlive this function if an operating system call
/// blocks. A process caller must exit after return. Cancellation flags detach
/// local waits and never request cancellation of durable remote work.
///
/// # Errors
///
/// Returns an I/O error if a stream worker cannot be started. Protocol failures
/// terminate the session after signalling every admitted local waiter.
pub fn serve(
    input: impl BufRead + Send + 'static,
    output: impl Write + Send + 'static,
    diagnostics: impl Write + Send + 'static,
    factory: impl Fn(Arc<AtomicBool>) -> Box<dyn ToolRunner>,
    process_stop: Arc<AtomicBool>,
) -> io::Result<()> {
    let stopping = Arc::new(AtomicBool::new(false));
    let (wake_sender, wake_receiver) = mpsc::sync_channel(1);
    let streams =
        Streams::start(input, output, diagnostics, wake_sender.clone(), Arc::clone(&stopping))?;
    let (completed_sender, completed_receiver) = mpsc::sync_channel(maximum_workers());
    let mut session = Session {
        application: ServerApplication::new(),
        streams,
        writing: None,
        held_input: None,
        input_ended: None,
        pressured: None,
        completed: completed_receiver,
        wake: wake_sender,
        completions: completed_sender,
    };
    while !process_stop.load(Ordering::SeqCst) && session.step(&factory) {
        let _ignored = wake_receiver.recv_timeout(COORDINATOR_POLL_INTERVAL);
    }
    stopping.store(true, Ordering::SeqCst);
    session.application.finish(OutputFailure::SinkFailed);
    Ok(())
}

/// An output response whose bytes and request reservation are still outstanding.
struct Delivery {
    /// Correlation released only on a complete write.
    identifier: Option<String>,
    /// Bytes charged while the writer owns the body.
    bytes: usize,
    /// Monotonic deadline independent of whether the writer itself is blocked.
    started: Instant,
}

/// State owned by the single coordinator, never shared with a worker.
struct Session {
    /// Handshake, reservations, pending execution and queued answers.
    application: ServerApplication,
    /// Owned input, output and diagnostic mailboxes.
    streams: Streams,
    /// The sole outstanding write.
    writing: Option<Delivery>,
    /// One bounded line awaiting the delivery of its previously used identifier.
    held_input: Option<BoundedLine>,
    /// EOF starts a bounded graceful drain.
    input_ended: Option<Instant>,
    /// A full response budget starts a bounded pressure wait.
    pressured: Option<Instant>,
    /// Worker results bounded by the number of admitted workers.
    completed: Receiver<Completion>,
    /// A coalesced wakeup for any stream or worker event.
    wake: SyncSender<()>,
    /// Sender cloned only for admitted workers.
    completions: SyncSender<Completion>,
}

impl Session {
    /// Makes progress without blocking on any stream or local operation.
    fn step(&mut self, factory: &impl Fn(Arc<AtomicBool>) -> Box<dyn ToolRunner>) -> bool {
        self.receive_delivery();
        while let Ok(completion) = self.completed.try_recv() {
            self.application.completed(completion);
        }
        if self.expired() || !self.application.output.accepts_more() {
            return false;
        }
        self.receive_input(factory);
        self.start_write();
        let records = self.application.take_diagnostics();
        if !records.is_empty() {
            let _ignored = self.streams.diagnostics.try_send(records);
        }
        !(self.input_ended.is_some()
            && self.application.active() == 0
            && self.application.output.waiting() == 0
            && self.writing.is_none())
    }

    /// Counts pending worker answers before allowing another input response.
    fn has_room(&self) -> bool {
        let reserved = self.application.pending.len().saturating_mul(maximum_line_bytes());
        let writing = self.writing.as_ref().map_or(0, |delivery| delivery.bytes);
        self.application
            .output
            .waiting_bytes()
            .saturating_add(reserved)
            .saturating_add(writing)
            .saturating_add(maximum_line_bytes())
            <= maximum_queued_bytes()
            && self.application.output.waiting() + self.application.pending.len() + 1
                < maximum_queued_messages()
    }

    /// Intake remains responsive while workers run, subject to output backpressure.
    fn receive_input(&mut self, factory: &impl Fn(Arc<AtomicBool>) -> Box<dyn ToolRunner>) {
        if self.input_ended.is_some() {
            return;
        }
        if !self.has_room() {
            self.pressured.get_or_insert_with(Instant::now);
            return;
        }
        self.pressured = None;
        let received =
            self.held_input.take().map(Ok).unwrap_or_else(|| self.streams.input.try_recv());
        match received {
            Ok(BoundedLine::Line(line)) => {
                if self.awaits_delivery(&line) {
                    self.held_input = Some(BoundedLine::Line(line));
                } else {
                    self.dispatch(&line, factory);
                }
            }
            Ok(BoundedLine::TooLong(line)) => {
                self.dispatch(&line, factory);
                self.input_ended = Some(Instant::now());
            }
            Ok(BoundedLine::End) | Err(TryRecvError::Disconnected) => {
                self.input_ended = Some(Instant::now());
            }
            Err(TryRecvError::Empty) => {}
        }
    }

    /// A peer may read the line before the writer's completion message arrives.
    /// Delay reuse of that one identifier until delivery settles, within the
    /// existing write deadline, instead of reporting a false duplicate.
    fn awaits_delivery(&self, line: &[u8]) -> bool {
        let Some(delivery) = &self.writing else {
            return false;
        };
        let Ok(Message::Request { identifier, .. }) = read_message(line) else {
            return false;
        };
        delivery.identifier.as_deref() == Some(identifier_key(&identifier).as_str())
    }

    /// Dispatches only a call for which the coordinator reserved output and identity.
    fn dispatch(&mut self, line: &[u8], factory: &impl Fn(Arc<AtomicBool>) -> Box<dyn ToolRunner>) {
        if let Some(request) = self.application.prepare_line(line, maximum_workers()) {
            let runner = factory(Arc::clone(&request.cancellation));
            self.start_worker(request, runner);
        }
    }

    /// Bounds worker lifetime by its cancellation flag; a failed start still settles.
    fn start_worker(&mut self, request: DeferredRequest, runner: Box<dyn ToolRunner>) {
        let fallback = request.failed();
        let failed_start = request.failed();
        let completed = self.completions.clone();
        let wake = self.wake.clone();
        let started =
            std::thread::Builder::new().name("protocol-request".to_owned()).spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    request.execute(runner)
                }))
                .unwrap_or(fallback);
                if completed.send(result).is_ok() {
                    let _ignored = wake.try_send(());
                }
            });
        if started.is_err() {
            self.application.completed(failed_start);
        }
    }

    /// Moves a body to the sole writer while retaining only delivery accounting.
    fn start_write(&mut self) {
        if self.writing.is_some() {
            return;
        }
        if let Some(queued) = self.application.output.take_next() {
            self.writing = Some(Delivery {
                identifier: queued.acknowledged_request,
                bytes: queued.line.len(),
                started: Instant::now(),
            });
            if self.streams.output.try_send(queued.line).is_err() {
                self.application.output.fail(OutputFailure::SinkFailed);
            }
        }
    }

    /// Releases a response reservation only after all bytes reached the stream.
    fn receive_delivery(&mut self) {
        match self.streams.delivered.try_recv() {
            Ok(Written::Complete) => {
                if let Some(delivery) = self.writing.take()
                    && let Some(identifier) = delivery.identifier
                {
                    self.application.active.acknowledged(&identifier);
                }
            }
            Ok(Written::Expired) => self.application.output.fail(OutputFailure::WriteExpired),
            Ok(_) | Err(TryRecvError::Disconnected) => {
                self.application.output.fail(OutputFailure::SinkFailed)
            }
            Err(TryRecvError::Empty) => {}
        }
    }

    /// Enforces deadlines even when a native stream operation does not return.
    fn expired(&self) -> bool {
        self.writing.as_ref().is_some_and(|delivery| delivery.started.elapsed() >= write_deadline())
            || self.input_ended.is_some_and(|started| started.elapsed() >= shutdown_deadline())
            || self.pressured.is_some_and(|started| started.elapsed() >= queue_pressure_deadline())
    }
}
