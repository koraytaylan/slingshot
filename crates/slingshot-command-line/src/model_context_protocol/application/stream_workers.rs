//! Owned standard streams behind bounded mailboxes.
//!
//! These threads are never joined during shutdown: an operating system read or
//! write can remain blocked after the peer has stopped participating. The
//! process owns their lifetime; the coordinator owns the shutdown deadline.

use std::io::{self, BufRead, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::time::{Duration, Instant};

use crate::model_context_protocol::standard_stream_transport::{
    BoundedLine, Written, read_bounded_line, write_deadline,
};

/// How often a nonblocking stream retries a write while checking termination.
const WRITE_RETRY_INTERVAL: Duration = Duration::from_millis(1);

/// Stream mailboxes owned exclusively by the coordinator.
pub(super) struct Streams {
    /// At most one line waiting, plus one being read.
    pub(super) input: Receiver<BoundedLine>,
    /// At most one response handed to the sole output writer.
    pub(super) output: SyncSender<String>,
    /// One delivery outcome; no retained response history.
    pub(super) delivered: Receiver<Written>,
    /// One bounded diagnostic batch waiting behind the one being written.
    pub(super) diagnostics: SyncSender<Vec<String>>,
}

impl Streams {
    /// Starts owned stream workers, returning an error if a thread cannot start.
    pub(super) fn start(
        input: impl BufRead + Send + 'static,
        output: impl Write + Send + 'static,
        diagnostics: impl Write + Send + 'static,
        wake: SyncSender<()>,
        stopping: Arc<AtomicBool>,
    ) -> io::Result<Self> {
        let (input_sender, input_receiver) = mpsc::sync_channel(1);
        let (output_sender, output_receiver) = mpsc::sync_channel(1);
        let (delivered_sender, delivered_receiver) = mpsc::sync_channel(1);
        let (diagnostic_sender, diagnostic_receiver) = mpsc::sync_channel(1);
        let output_wake = wake.clone();
        let output_stopping = Arc::clone(&stopping);
        std::thread::Builder::new().name("protocol-output".to_owned()).spawn(move || {
            write_responses(
                output,
                output_receiver,
                delivered_sender,
                output_wake,
                output_stopping,
            );
        })?;
        std::thread::Builder::new().name("protocol-diagnostics".to_owned()).spawn(move || {
            write_diagnostics(diagnostics, diagnostic_receiver, stopping);
        })?;
        std::thread::Builder::new().name("protocol-input".to_owned()).spawn(move || {
            read_lines(input, input_sender, wake);
        })?;
        Ok(Self {
            input: input_receiver,
            output: output_sender,
            delivered: delivered_receiver,
            diagnostics: diagnostic_sender,
        })
    }
}

/// Reads through a bounded mailbox; a hostile oversized line ends intake.
fn read_lines(mut input: impl BufRead, sender: SyncSender<BoundedLine>, wake: SyncSender<()>) {
    loop {
        let line = read_bounded_line(&mut input).unwrap_or(BoundedLine::End);
        let terminal = !matches!(line, BoundedLine::Line(_));
        if sender.send(line).is_err() {
            return;
        }
        let _ignored = wake.try_send(());
        if terminal {
            return;
        }
    }
}

/// Writes exactly one response at a time and reports only its delivery outcome.
fn write_responses(
    mut output: impl Write,
    receiver: Receiver<String>,
    delivered: SyncSender<Written>,
    wake: SyncSender<()>,
    stopping: Arc<AtomicBool>,
) {
    while let Ok(mut line) = receiver.recv() {
        line.push('\n');
        let outcome = write_line(&mut output, line.as_bytes(), &stopping);
        drop(line);
        if delivered.send(outcome).is_err() {
            return;
        }
        let _ignored = wake.try_send(());
        if outcome != Written::Complete {
            return;
        }
    }
}

/// Delivers one complete line while checking the local monotonic write budget.
fn write_line(output: &mut impl Write, mut remaining: &[u8], stopping: &AtomicBool) -> Written {
    let started = Instant::now();
    while !remaining.is_empty() {
        if stopping.load(Ordering::SeqCst) || started.elapsed() >= write_deadline() {
            return Written::Expired;
        }
        match output.write(remaining) {
            Ok(0) => return Written::Refused,
            Ok(written) => remaining = &remaining[written..],
            Err(failure) if failure.kind() == io::ErrorKind::Interrupted => {}
            Err(failure) if failure.kind() == io::ErrorKind::WouldBlock => {
                std::thread::sleep(WRITE_RETRY_INTERVAL);
            }
            Err(_) => return Written::Refused,
        }
    }
    if output.flush().is_err() { Written::Refused } else { Written::Complete }
}

/// Diagnostic congestion never blocks the coordinator or its protocol writer.
fn write_diagnostics(
    mut output: impl Write,
    receiver: Receiver<Vec<String>>,
    stopping: Arc<AtomicBool>,
) {
    while let Ok(records) = receiver.recv() {
        for record in records {
            if stopping.load(Ordering::SeqCst) || writeln!(output, "slingshot: {record}").is_err() {
                return;
            }
        }
    }
}
