//! Single-use, full-duplex finite HTTP/2 exchanges on the immutable author.
//! No retry, redirect, protocol fallback, pooled connection or partial receipt.

use core::future::Future;
use core::{
    pin::Pin,
    task::{Context, Poll},
};
use http::{HeaderMap, Method};
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt, ReadBuf};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::time::{Duration, Instant, Sleep, sleep, sleep_until, timeout_at};

use crate::authentication::environment_provider::RequestAuthentication;
use crate::author_hypertext_transfer_protocol_policy::ExchangeDeadlines;
use crate::selected_author_exchange::SelectedAuthorFiniteResponse;
use crate::selected_author_http::{FiniteHttpFailure, FiniteHttpReceipt};
use crate::selected_author_http2_flow::SendWindows;
use crate::selected_author_http2_frames::{FrameRead, ResponseFrame, ResponseFrameReader};
use crate::selected_author_http2_handshake::Negotiated;
use crate::selected_author_http2_response::FiniteResponse;
use crate::selected_author_transport::SelectedAuthorTransport;

const DATA_FRAME: u8 = 0;
const HEADERS_FRAME: u8 = 1;
const RESET_FRAME: u8 = 3;
const SETTINGS_FRAME: u8 = 4;
const PING_FRAME: u8 = 6;
const GOAWAY_FRAME: u8 = 7;
const WINDOW_UPDATE_FRAME: u8 = 8;
const CONTINUATION_FRAME: u8 = 9;
const ACKNOWLEDGEMENT_FLAG: u8 = 1;
const PING_FRAME_BYTES: usize = 17;
const WINDOW_UPDATE_FRAME_BYTES: usize = 13;
const FLOW_CONTROL_WINDOWS: usize = 2;
const NANOSECONDS_PER_MILLISECOND: u128 = 1_000_000;

/// One receive-window update for the connection and one for its single stream.
pub(crate) type FlowCredits = [[u8; WINDOW_UPDATE_FRAME_BYTES]; FLOW_CONTROL_WINDOWS];

impl SelectedAuthorTransport {
    /// Sends one finite GET/POST after strict selected-author h2 negotiation.
    /// Callers still own durable submission authority and route identity checks.
    ///
    /// # Errors
    /// Refuses invalid request/authentication bindings, failed negotiation or
    /// writes, malformed or incomplete responses, and expired exchange deadlines.
    /// Failures after sending do not establish remote nonexecution.
    pub async fn finite_http2_query(
        &self,
        method: Method,
        segments: &[&str],
        query: &[(&str, &str)],
        authentication: &RequestAuthentication,
        fields: &HeaderMap,
        body: &[u8],
    ) -> Result<FiniteHttpReceipt, FiniteHttpFailure> {
        let head =
            self.encode_http2_request_head(method, segments, query, authentication, fields, body)?;
        let started = Instant::now();
        let prepared = self.prepare_http2().await?;
        let response = drive(
            prepared.stream,
            prepared.negotiated,
            head.frames(),
            body,
            ExchangeDeadlines::embedded(),
        )
        .await?;
        Ok(FiniteHttpReceipt {
            response,
            elapsed_milliseconds: u64::try_from(
                started.elapsed().as_nanos().div_ceil(NANOSECONDS_PER_MILLISECOND),
            )
            .unwrap_or(u64::MAX),
        })
    }
}

// Only one bounded frame/control message may wait between the two futures.
// try_join drops both halves on failure/cancellation; no detached worker can
// keep sending after its caller has gone away.
enum Outgoing {
    Control(ResponseFrame),
    Credit(FlowCredits, oneshot::Sender<()>),
    Finish,
}

/// A private response consumer: transport completion remains owned by the driver.
/// Streaming sinks may stage bytes, but only finish can return completion evidence.
pub(crate) trait ResponseConsumer {
    type Output;
    fn head_complete(&self) -> bool;
    fn stream_ended(&self) -> bool;
    fn stream_end_is_terminal(&self) -> bool {
        false
    }
    fn accept(
        &mut self,
        frame: &ResponseFrame,
    ) -> Result<Option<FlowCredits>, crate::selected_author_http2_response::ResponseRefusal>;
    fn finish_at_transport_end(
        self,
        end: crate::selected_author_http2_frames::TransportEnd,
    ) -> Result<Self::Output, crate::selected_author_http2_response::ResponseRefusal>;
    fn finish_at_stream_end(
        self,
    ) -> Result<Self::Output, crate::selected_author_http2_response::ResponseRefusal>
    where
        Self: Sized,
    {
        Err(crate::selected_author_http2_response::ResponseRefusal)
    }
    fn body_deadlines(&self, defaults: ExchangeDeadlines) -> (u64, u64) {
        (defaults.finite_total_milliseconds, defaults.finite_idle_milliseconds)
    }
    /// Live consumers supply a deadline renewed only by complete validated
    /// stream items. They have neither a finite total nor a raw-byte idle limit.
    /// Once supplied at head completion this must remain Some until disposal.
    fn liveness_deadline(&self) -> Option<Instant> {
        None
    }
}

impl ResponseConsumer for FiniteResponse {
    type Output = SelectedAuthorFiniteResponse;
    fn head_complete(&self) -> bool {
        self.head_complete()
    }
    fn stream_ended(&self) -> bool {
        self.stream_ended()
    }
    fn stream_end_is_terminal(&self) -> bool {
        true
    }
    fn accept(
        &mut self,
        frame: &ResponseFrame,
    ) -> Result<Option<FlowCredits>, crate::selected_author_http2_response::ResponseRefusal> {
        self.accept(frame)
    }
    fn finish_at_transport_end(
        self,
        end: crate::selected_author_http2_frames::TransportEnd,
    ) -> Result<Self::Output, crate::selected_author_http2_response::ResponseRefusal> {
        self.finish_at_transport_end(end)
    }
    fn finish_at_stream_end(
        self,
    ) -> Result<Self::Output, crate::selected_author_http2_response::ResponseRefusal> {
        FiniteResponse::finish_at_stream_end(self)
    }
}

pub(crate) async fn drive(
    stream: impl AsyncRead + AsyncWrite + Unpin,
    negotiated: Negotiated,
    head: impl Iterator<Item = Vec<u8>>,
    body: &[u8],
    deadlines: ExchangeDeadlines,
) -> Result<SelectedAuthorFiniteResponse, FiniteHttpFailure> {
    drive_response(stream, negotiated, head, body, deadlines, FiniteResponse::new()).await
}

pub(crate) async fn drive_response<Consumer: ResponseConsumer>(
    stream: impl AsyncRead + AsyncWrite + Unpin,
    negotiated: Negotiated,
    head: impl Iterator<Item = Vec<u8>>,
    body: &[u8],
    deadlines: ExchangeDeadlines,
    response: Consumer,
) -> Result<Consumer::Output, FiniteHttpFailure> {
    let (input, output) = tokio::io::split(stream);
    let (commands, pending) = mpsc::channel(1);
    let (completed, completion) = watch::channel(None);
    let started = Instant::now();
    let request_end = started + Duration::from_millis(deadlines.request_body_milliseconds);
    let writer =
        write(output, negotiated.send_windows, head, body, pending, completed, request_end);
    let reader =
        read(input, negotiated.frames, commands, completion, request_end, deadlines, response);
    let ((), response) = tokio::try_join!(writer, reader)?;
    Ok(response)
}

async fn write(
    mut output: impl AsyncWrite + Unpin,
    mut windows: SendWindows,
    head: impl Iterator<Item = Vec<u8>>,
    body: &[u8],
    mut commands: mpsc::Receiver<Outgoing>,
    completed: watch::Sender<Option<Instant>>,
    request_end: Instant,
) -> Result<(), FiniteHttpFailure> {
    let mut position = 0;
    let early_end = timeout_at(request_end, async {
        // HEADERS/CONTINUATION are one uninterrupted block.
        for frame in head {
            send(&mut output, &frame).await?;
        }
        while position < body.len() {
            match commands.try_recv() {
                Ok(Outgoing::Finish) => return Ok(true),
                Ok(command) => {
                    control(&mut output, &mut windows, command, false).await?;
                    continue;
                }
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    return Err(FiniteHttpFailure::Write);
                }
                Err(mpsc::error::TryRecvError::Empty) => {}
            }
            let count = windows.reserve(body.len() - position);
            if count == 0 {
                match commands.recv().await.ok_or(FiniteHttpFailure::Write)? {
                    Outgoing::Finish => return Ok(true),
                    command => control(&mut output, &mut windows, command, false).await?,
                }
                continue;
            }
            let last = position + count == body.len();
            let length = (count as u32).to_be_bytes();
            let header = [length[1], length[2], length[3], 0, u8::from(last), 0, 0, 0, 1];
            output.write_all(&header).await.map_err(|_| FiniteHttpFailure::Write)?;
            send(&mut output, &body[position..position + count]).await?;
            position += count;
        }
        Ok(false)
    })
    .await
    .map_err(|_| FiniteHttpFailure::Write)??;
    if !early_end {
        let _ = completed.send(Some(Instant::now()));
        finish_controls(&mut output, &mut windows, &mut commands).await?;
    }
    if position < body.len() {
        // A complete early response can stop the unfinished request half.
        send(&mut output, &[0, 0, 4, 3, 0, 0, 0, 0, 1, 0, 0, 0, 0])
            .await
            .map_err(|_| FiniteHttpFailure::Body)?;
    }
    // Single-use connection: ask for closure, then prove clean peer EOF.
    // The client has accepted no peer-initiated streams (push is disabled).
    send(&mut output, &[0, 0, 8, 7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])
        .await
        .map_err(|_| FiniteHttpFailure::Body)?;
    output.shutdown().await.map_err(|_| FiniteHttpFailure::Body)
}

async fn finish_controls(
    output: &mut (impl AsyncWrite + Unpin),
    windows: &mut SendWindows,
    commands: &mut mpsc::Receiver<Outgoing>,
) -> Result<(), FiniteHttpFailure> {
    loop {
        match commands.recv().await.ok_or(FiniteHttpFailure::Body)? {
            Outgoing::Finish => return Ok(()),
            command => control(output, windows, command, true)
                .await
                .map_err(|_| FiniteHttpFailure::Body)?,
        }
    }
}

async fn send(
    output: &mut (impl AsyncWrite + Unpin),
    bytes: &[u8],
) -> Result<(), FiniteHttpFailure> {
    output.write_all(bytes).await.map_err(|_| FiniteHttpFailure::Write)?;
    output.flush().await.map_err(|_| FiniteHttpFailure::Write)
}

async fn control(
    output: &mut (impl AsyncWrite + Unpin),
    windows: &mut SendWindows,
    command: Outgoing,
    request_complete: bool,
) -> Result<(), FiniteHttpFailure> {
    match command {
        Outgoing::Control(frame) => match frame.kind {
            SETTINGS_FRAME => {
                windows.observe(&frame).map_err(|_| FiniteHttpFailure::Body)?;
                send(output, &[0, 0, 0, 4, 1, 0, 0, 0, 0]).await?;
            }
            WINDOW_UPDATE_FRAME if request_complete && frame.stream_identifier == 1 => {}
            WINDOW_UPDATE_FRAME => windows.observe(&frame).map_err(|_| FiniteHttpFailure::Body)?,
            PING_FRAME => {
                let mut ack = [0; PING_FRAME_BYTES];
                ack[..9].copy_from_slice(&[0, 0, 8, 6, 1, 0, 0, 0, 0]);
                ack[9..].copy_from_slice(&frame.payload);
                send(output, &ack).await?;
            }
            _ => return Err(FiniteHttpFailure::Body),
        },
        Outgoing::Credit(frames, consumed) => {
            for frame in frames {
                send(output, &frame).await?;
            }
            let _ = consumed.send(());
        }
        Outgoing::Finish => return Err(FiniteHttpFailure::Body),
    }
    Ok(())
}

async fn read<Consumer: ResponseConsumer>(
    input: impl AsyncRead + Unpin,
    mut frames: ResponseFrameReader,
    commands: mpsc::Sender<Outgoing>,
    mut completion: watch::Receiver<Option<Instant>>,
    request_end: Instant,
    deadlines: ExchangeDeadlines,
    mut response: Consumer,
) -> Result<Consumer::Output, FiniteHttpFailure> {
    let mut input = IdleRead::new(input);
    let mut timing =
        ReadTiming { request_end, deadlines, body_end: None, live: false, body_started: false };
    let mut finish_sent = false;
    loop {
        let failure = if response.head_complete() {
            FiniteHttpFailure::Body
        } else {
            FiniteHttpFailure::Head
        };
        let frame = timing
            .next_frame(&mut input, &mut frames, &mut completion, &mut response, failure)
            .await?;
        match frame {
            FrameRead::End(end) => {
                return response.finish_at_transport_end(end).map_err(|_| failure);
            }
            FrameRead::Frame(frame) => {
                let PendingFrame { command, credit_written } =
                    classify_response_frame(frame, &mut response, timing.live, failure)?;
                timing.begin_body(&mut input, &response);
                let end = timing.end(&response, *completion.borrow())?;
                // Credit must reach the writer before more DATA is admitted.
                let timeout_failure = timing.timeout_failure(failure);
                if let Some(command) = command {
                    if !response.stream_ended() {
                        timeout_at(end, commands.send(command))
                            .await
                            .map_err(|_| timeout_failure)?
                            .map_err(|_| failure)?;
                    }
                }
                if let Some(written) = credit_written {
                    timeout_at(end, written)
                        .await
                        .map_err(|_| timeout_failure)?
                        .map_err(|_| failure)?;
                }
                if response.stream_ended() && !finish_sent {
                    timeout_at(end, commands.send(Outgoing::Finish))
                        .await
                        .map_err(|_| timeout_failure)?
                        .map_err(|_| failure)?;
                    finish_sent = true;
                    if response.stream_end_is_terminal() {
                        return finish_response_stream(response, &mut frames, &mut input, failure)
                            .await;
                    }
                }
            }
        }
    }
}

struct ReadTiming {
    request_end: Instant,
    deadlines: ExchangeDeadlines,
    body_end: Option<Instant>,
    live: bool,
    body_started: bool,
}

impl ReadTiming {
    fn end(
        &self,
        response: &impl ResponseConsumer,
        completed: Option<Instant>,
    ) -> Result<Instant, FiniteHttpFailure> {
        if self.live {
            response.liveness_deadline().ok_or(FiniteHttpFailure::Body)
        } else {
            Ok(self.body_end.unwrap_or_else(|| {
                completed.unwrap_or(self.request_end)
                    + Duration::from_millis(self.deadlines.response_header_milliseconds)
            }))
        }
    }

    fn timeout_failure(&self, failure: FiniteHttpFailure) -> FiniteHttpFailure {
        if self.live { FiniteHttpFailure::EventHeartbeat } else { failure }
    }

    fn begin_body<Reader>(
        &mut self,
        input: &mut IdleRead<Reader>,
        response: &impl ResponseConsumer,
    ) {
        if response.head_complete() && !self.body_started {
            self.body_started = true;
            self.live = response.liveness_deadline().is_some();
            if !self.live {
                let (total, idle) = response.body_deadlines(self.deadlines);
                self.body_end = Some(Instant::now() + Duration::from_millis(total));
                input.enable(Duration::from_millis(idle));
            }
        }
    }

    async fn next_frame(
        &self,
        input: &mut (impl AsyncRead + Unpin),
        frames: &mut ResponseFrameReader,
        completion: &mut watch::Receiver<Option<Instant>>,
        response: &mut impl ResponseConsumer,
        failure: FiniteHttpFailure,
    ) -> Result<FrameRead, FiniteHttpFailure> {
        let next = frames.read_next(input);
        tokio::pin!(next);
        // Request completion changes the head deadline without cancelling
        // the in-progress frame read, which would poison framing state.
        loop {
            let completed = *completion.borrow();
            let end = self.end(response, completed)?;
            let expired = self.timeout_failure(failure);
            if Instant::now() >= end {
                return Err(expired);
            }
            tokio::select! {
                result = &mut next => {
                    if Instant::now() >= end { return Err(expired); }
                    return result.map_err(|_| failure);
                },
                changed = completion.changed(), if !self.body_started && completed.is_none() => {
                    changed.map_err(|_| failure)?;
                }
                _ = sleep_until(end) => return Err(expired),
            }
        }
    }
}

// END_STREAM can complete without peer EOF. Drain frames already available
// first, so a trailing response frame on stream 1 cannot become published evidence.
async fn finish_response_stream<Consumer: ResponseConsumer>(
    response: Consumer,
    frames: &mut ResponseFrameReader,
    input: &mut (impl AsyncRead + Unpin),
    failure: FiniteHttpFailure,
) -> Result<Consumer::Output, FiniteHttpFailure> {
    loop {
        let next = frames.read_next(input);
        tokio::pin!(next);
        let next = tokio::select! {
            biased;
            result = &mut next => Some(result.map_err(|_| failure)?),
            _ = tokio::task::yield_now() => None,
        };
        let Some(next) = next else {
            return response.finish_at_stream_end().map_err(|_| failure);
        };
        match next {
            FrameRead::End(_) => return response.finish_at_stream_end().map_err(|_| failure),
            FrameRead::Frame(frame)
                if frame.stream_identifier == 1
                    && matches!(frame.kind, DATA_FRAME | HEADERS_FRAME | CONTINUATION_FRAME) =>
            {
                return Err(failure);
            }
            FrameRead::Frame(_) => {}
        }
    }
}

struct PendingFrame {
    command: Option<Outgoing>,
    credit_written: Option<oneshot::Receiver<()>>,
}

fn classify_response_frame(
    frame: ResponseFrame,
    response: &mut impl ResponseConsumer,
    live: bool,
    failure: FiniteHttpFailure,
) -> Result<PendingFrame, FiniteHttpFailure> {
    let mut credit_written = None;
    let command = match frame.kind {
        DATA_FRAME | HEADERS_FRAME | CONTINUATION_FRAME => {
            let credits =
                response.accept(&frame).map_err(|_| response_failure(response, live, failure))?;
            if response.stream_ended() {
                None
            } else {
                credits.map(|frames| {
                    let (written, confirmation) = oneshot::channel();
                    credit_written = Some(confirmation);
                    Outgoing::Credit(frames, written)
                })
            }
        }
        RESET_FRAME => return Err(failure),
        SETTINGS_FRAME if frame.flags & ACKNOWLEDGEMENT_FLAG != 0 => return Err(failure), // our sole SETTINGS was already ACKed
        SETTINGS_FRAME | WINDOW_UPDATE_FRAME => Some(Outgoing::Control(frame)),
        PING_FRAME if frame.flags & ACKNOWLEDGEMENT_FLAG == 0 => Some(Outgoing::Control(frame)),
        GOAWAY_FRAME => {
            require_successful_goaway(&frame).map_err(|_| failure)?;
            None
        }
        _ => None,
    };
    Ok(PendingFrame { command, credit_written })
}

fn response_failure(
    response: &impl ResponseConsumer,
    live: bool,
    failure: FiniteHttpFailure,
) -> FiniteHttpFailure {
    if live && response.liveness_deadline().is_some_and(|end| Instant::now() >= end) {
        FiniteHttpFailure::EventHeartbeat
    } else {
        failure
    }
}

fn require_successful_goaway(frame: &ResponseFrame) -> Result<(), FiniteHttpFailure> {
    const STREAM_IDENTIFIER_MASK: u32 = 0x7fff_ffff;
    let last = u32::from_be_bytes(frame.payload[..4].try_into().unwrap()) & STREAM_IDENTIFIER_MASK;
    let error = u32::from_be_bytes(frame.payload[4..8].try_into().unwrap());
    if last < 1 || error != 0 {
        return Err(FiniteHttpFailure::Body);
    }
    Ok(())
}

// Idle means time between received bytes, not time to collect a whole frame.
struct IdleRead<Reader> {
    input: Reader,
    duration: Option<Duration>,
    timer: Pin<Box<Sleep>>,
}

impl<Reader> IdleRead<Reader> {
    fn new(input: Reader) -> Self {
        Self { input, duration: None, timer: Box::pin(sleep(Duration::ZERO)) }
    }
    fn enable(&mut self, duration: Duration) {
        self.duration = Some(duration);
        self.timer.as_mut().reset(Instant::now() + duration);
    }
}

impl<Reader: AsyncRead + Unpin> AsyncRead for IdleRead<Reader> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        if self.duration.is_some() && self.timer.as_mut().poll(context).is_ready() {
            return Poll::Ready(Err(std::io::ErrorKind::TimedOut.into()));
        }
        let before = buffer.filled().len();
        let result = Pin::new(&mut self.input).poll_read(context, buffer);
        if let Some(duration) = self.duration {
            if matches!(result, Poll::Ready(Ok(()))) && buffer.filled().len() > before {
                self.timer.as_mut().reset(Instant::now() + duration);
            }
        }
        result
    }
}

#[cfg(test)]
#[path = "selected_author_http2_tests.rs"]
pub(crate) mod tests;
