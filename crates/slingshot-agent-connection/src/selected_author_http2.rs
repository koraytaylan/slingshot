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

impl SelectedAuthorTransport {
    /// Sends one finite GET/POST after strict selected-author h2 negotiation.
    /// Callers still own durable submission authority and route identity checks.
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
            elapsed_milliseconds: u64::try_from(started.elapsed().as_nanos().div_ceil(1_000_000))
                .unwrap_or(u64::MAX),
        })
    }
}

// Only one bounded frame/control message may wait between the two futures.
// try_join drops both halves on failure/cancellation; no detached worker can
// keep sending after its caller has gone away.
enum Outgoing {
    Control(ResponseFrame),
    Credit([[u8; 13]; 2], oneshot::Sender<()>),
    Finish,
}

/// A private response consumer: transport completion remains owned by the driver.
/// Streaming sinks may stage bytes, but only finish can return completion evidence.
pub(crate) trait ResponseConsumer {
    type Output;
    fn head_complete(&self) -> bool;
    fn stream_ended(&self) -> bool;
    fn accept(
        &mut self,
        frame: &ResponseFrame,
    ) -> Result<Option<[[u8; 13]; 2]>, crate::selected_author_http2_response::ResponseRefusal>;
    fn finish_at_transport_end(
        self,
        end: crate::selected_author_http2_frames::TransportEnd,
    ) -> Result<Self::Output, crate::selected_author_http2_response::ResponseRefusal>;
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
    fn accept(
        &mut self,
        frame: &ResponseFrame,
    ) -> Result<Option<[[u8; 13]; 2]>, crate::selected_author_http2_response::ResponseRefusal> {
        self.accept(frame)
    }
    fn finish_at_transport_end(
        self,
        end: crate::selected_author_http2_frames::TransportEnd,
    ) -> Result<Self::Output, crate::selected_author_http2_response::ResponseRefusal> {
        self.finish_at_transport_end(end)
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

pub(crate) async fn drive_response<R: ResponseConsumer>(
    stream: impl AsyncRead + AsyncWrite + Unpin,
    negotiated: Negotiated,
    head: impl Iterator<Item = Vec<u8>>,
    body: &[u8],
    deadlines: ExchangeDeadlines,
    response: R,
) -> Result<R::Output, FiniteHttpFailure> {
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
        loop {
            match commands.recv().await.ok_or(FiniteHttpFailure::Body)? {
                Outgoing::Finish => break,
                command => control(&mut output, &mut windows, command, true)
                    .await
                    .map_err(|_| FiniteHttpFailure::Body)?,
            }
        }
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
            4 => {
                windows.observe(&frame).map_err(|_| FiniteHttpFailure::Body)?;
                send(output, &[0, 0, 0, 4, 1, 0, 0, 0, 0]).await?;
            }
            8 if request_complete && frame.stream_identifier == 1 => {}
            8 => windows.observe(&frame).map_err(|_| FiniteHttpFailure::Body)?,
            6 => {
                let mut ack = [0; 17];
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

async fn read<R: ResponseConsumer>(
    input: impl AsyncRead + Unpin,
    mut frames: ResponseFrameReader,
    commands: mpsc::Sender<Outgoing>,
    mut completion: watch::Receiver<Option<Instant>>,
    request_end: Instant,
    deadlines: ExchangeDeadlines,
    mut response: R,
) -> Result<R::Output, FiniteHttpFailure> {
    let mut input = IdleRead::new(input);
    let mut body_end = None;
    let mut live = false;
    let mut body_started = false;
    let mut finish_sent = false;
    loop {
        let failure = if response.head_complete() {
            FiniteHttpFailure::Body
        } else {
            FiniteHttpFailure::Head
        };
        let timeout_failure = if live { FiniteHttpFailure::EventHeartbeat } else { failure };
        let frame = {
            let next = frames.read_next(&mut input);
            tokio::pin!(next);
            // A completed request changes the head deadline without cancelling an
            // in-progress frame read (which would poison the framing reader).
            loop {
                let request_completed = *completion.borrow();
                let end = if live {
                    response.liveness_deadline().ok_or(FiniteHttpFailure::Body)?
                } else {
                    body_end.unwrap_or_else(|| {
                        request_completed.unwrap_or(request_end)
                            + Duration::from_millis(deadlines.response_header_milliseconds)
                    })
                };
                if Instant::now() >= end {
                    return Err(timeout_failure);
                }
                tokio::select! {
                    result = &mut next => {
                        if Instant::now() >= end { return Err(timeout_failure); }
                        break result.map_err(|_| failure)?;
                    },
                    changed = completion.changed(), if !body_started && request_completed.is_none() => {
                        changed.map_err(|_| failure)?;
                    }
                    _ = sleep_until(end) => return Err(timeout_failure),
                }
            }
        };
        match frame {
            FrameRead::End(end) => {
                return response.finish_at_transport_end(end).map_err(|_| failure);
            }
            FrameRead::Frame(frame) => {
                let mut credit_written = None;
                let command = match frame.kind {
                    0 | 1 | 9 => {
                        let credits = response.accept(&frame).map_err(|_| {
                            if live
                                && response
                                    .liveness_deadline()
                                    .is_some_and(|end| Instant::now() >= end)
                            {
                                FiniteHttpFailure::EventHeartbeat
                            } else {
                                failure
                            }
                        })?;
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
                    3 => return Err(failure),
                    4 if frame.flags & 1 != 0 => return Err(failure), // our sole SETTINGS was already ACKed
                    4 | 8 => Some(Outgoing::Control(frame)),
                    6 if frame.flags & 1 == 0 => Some(Outgoing::Control(frame)),
                    7 => {
                        let last = u32::from_be_bytes(frame.payload[..4].try_into().unwrap())
                            & 0x7fff_ffff;
                        let error = u32::from_be_bytes(frame.payload[4..8].try_into().unwrap());
                        if last < 1 || error != 0 {
                            return Err(failure);
                        }
                        None
                    }
                    _ => None,
                };
                if response.head_complete() && !body_started {
                    body_started = true;
                    live = response.liveness_deadline().is_some();
                    if !live {
                        let (total, idle) = response.body_deadlines(deadlines);
                        body_end = Some(Instant::now() + Duration::from_millis(total));
                        input.enable(Duration::from_millis(idle));
                    }
                }
                let end = if live {
                    response.liveness_deadline().ok_or(FiniteHttpFailure::Body)?
                } else {
                    body_end.unwrap_or_else(|| {
                        (*completion.borrow()).unwrap_or(request_end)
                            + Duration::from_millis(deadlines.response_header_milliseconds)
                    })
                };
                // Credit must reach the writer before more DATA is admitted.
                let timeout_failure =
                    if live { FiniteHttpFailure::EventHeartbeat } else { failure };
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
                }
            }
        }
    }
}

// Idle means time between received bytes, not time to collect a whole frame.
struct IdleRead<R> {
    input: R,
    duration: Option<Duration>,
    timer: Pin<Box<Sleep>>,
}

impl<R> IdleRead<R> {
    fn new(input: R) -> Self {
        Self { input, duration: None, timer: Box::pin(sleep(Duration::ZERO)) }
    }
    fn enable(&mut self, duration: Duration) {
        self.duration = Some(duration);
        self.timer.as_mut().reset(Instant::now() + duration);
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for IdleRead<R> {
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
pub(crate) mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, DuplexStream};
    use tokio::time::timeout;

    pub(crate) fn frame(kind: u8, flags: u8, stream: u32, payload: &[u8]) -> Vec<u8> {
        let length = (payload.len() as u32).to_be_bytes();
        let mut result = vec![length[1], length[2], length[3], kind, flags];
        result.extend_from_slice(&stream.to_be_bytes());
        result.extend_from_slice(payload);
        result
    }

    pub(crate) async fn receive(peer: &mut DuplexStream) -> ResponseFrame {
        let mut header = [0; 9];
        peer.read_exact(&mut header).await.unwrap();
        let length =
            usize::from(header[0]) << 16 | usize::from(header[1]) << 8 | usize::from(header[2]);
        assert!(length <= 16_384);
        let mut payload = vec![0; length];
        peer.read_exact(&mut payload).await.unwrap();
        ResponseFrame {
            kind: header[3],
            flags: header[4],
            stream_identifier: u32::from_be_bytes(header[5..].try_into().unwrap()),
            payload,
        }
    }

    pub(crate) async fn handshake(peer: &mut DuplexStream, window: u32) {
        let mut preface = [0; 39];
        peer.read_exact(&mut preface).await.unwrap();
        assert_eq!(&preface[..24], b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n");
        let mut setting = vec![0, 4];
        setting.extend_from_slice(&window.to_be_bytes());
        peer.write_all(&frame(4, 0, 0, &setting)).await.unwrap();
        let ack = receive(peer).await;
        assert_eq!((ack.kind, ack.flags), (4, 1));
        peer.write_all(&frame(4, 1, 0, &[])).await.unwrap();
    }

    fn response_head(ended: bool) -> Vec<u8> {
        let mut block = vec![0x88, 0x0f, 16, 16]; // :status 200; literal static name 31 content-type
        block.extend_from_slice(b"application/json");
        frame(1, if ended { 5 } else { 4 }, 1, &block)
    }

    pub(crate) fn deadlines() -> ExchangeDeadlines {
        ExchangeDeadlines {
            connect_milliseconds: 1000,
            transport_layer_security_milliseconds: 1000,
            request_body_milliseconds: 1000,
            response_header_milliseconds: 1000,
            finite_idle_milliseconds: 1000,
            finite_total_milliseconds: 3000,
        }
    }

    async fn client(
        mut stream: DuplexStream,
        body: &[u8],
        deadlines: ExchangeDeadlines,
    ) -> Result<SelectedAuthorFiniteResponse, FiniteHttpFailure> {
        let negotiated =
            crate::selected_author_http2_handshake::negotiate(&mut stream, Duration::from_secs(1))
                .await?;
        let block =
            [if body.is_empty() { 0x82 } else { 0x83 }, 0x86, 0x84, 1, 4, b't', b'e', b's', b't'];
        drive(
            stream,
            negotiated,
            [frame(1, if body.is_empty() { 5 } else { 4 }, 1, &block)].into_iter(),
            body,
            deadlines,
        )
        .await
    }

    pub(crate) async fn close(peer: &mut DuplexStream) {
        loop {
            let outgoing = receive(peer).await;
            if outgoing.kind == 7 {
                assert_eq!(outgoing.payload, [0; 8]);
                break;
            }
            assert!(matches!(outgoing.kind, 3 | 4 | 6 | 8));
        }
        assert_eq!(peer.read(&mut [0; 1]).await.unwrap(), 0);
        peer.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn full_duplex_flow_control_handles_request_and_response_larger_than_windows() {
        let (stream, mut peer) = tokio::io::duplex(1024);
        let body = vec![b'x'; 100_000];
        let server = async {
            handshake(&mut peer, 7).await;
            assert_eq!(receive(&mut peer).await.kind, 1);
            let first = receive(&mut peer).await;
            assert_eq!(first.payload, vec![b'x'; 7]);
            // No stream credit remains: the client must continue reading.
            peer.write_all(&frame(6, 0, 0, b"12345678")).await.unwrap();
            let ack = receive(&mut peer).await;
            assert_eq!(
                (ack.kind, ack.flags, ack.payload.as_slice()),
                (6, 1, b"12345678".as_slice())
            );
            peer.write_all(&frame(8, 0, 1, &100_000u32.to_be_bytes())).await.unwrap();
            peer.write_all(&frame(8, 0, 0, &100_000u32.to_be_bytes())).await.unwrap();
            let mut received = first.payload.clone();
            loop {
                let data = receive(&mut peer).await;
                assert_eq!(data.kind, 0);
                received.extend_from_slice(&data.payload);
                if data.flags & 1 != 0 {
                    break;
                }
            }
            assert_eq!(received, body);
            peer.write_all(&response_head(false)).await.unwrap();
            for index in 0..10 {
                peer.write_all(&frame(0, u8::from(index == 9), 1, &vec![b'y'; 10_000]))
                    .await
                    .unwrap();
                if index != 9 {
                    for stream in [0, 1] {
                        let credit = receive(&mut peer).await;
                        assert_eq!((credit.kind, credit.stream_identifier), (8, stream));
                        assert_eq!(credit.payload, 10_000u32.to_be_bytes());
                    }
                }
            }
            close(&mut peer).await;
        };
        let (result, ()) = timeout(Duration::from_secs(5), async {
            tokio::join!(client(stream, &body, deadlines()), server)
        })
        .await
        .unwrap();
        assert_eq!(result.unwrap().body, vec![b'y'; 100_000]);
    }

    #[tokio::test]
    async fn complete_early_response_closes_the_unfinished_request_without_retry() {
        let (stream, mut peer) = tokio::io::duplex(1024);
        let server = async {
            handshake(&mut peer, 0).await;
            assert_eq!(receive(&mut peer).await.kind, 1);
            peer.write_all(&response_head(true)).await.unwrap();
            let reset = receive(&mut peer).await;
            assert_eq!((reset.kind, reset.stream_identifier), (3, 1));
            assert_eq!(reset.payload, [0; 4]);
            close(&mut peer).await;
        };
        let (result, ()) = timeout(Duration::from_secs(2), async {
            tokio::join!(client(stream, b"not sent", deadlines()), server)
        })
        .await
        .unwrap();
        assert!(result.unwrap().body.is_empty());
    }

    #[tokio::test]
    async fn post_end_control_frames_do_not_issue_multiple_close_commands() {
        let (stream, mut peer) = tokio::io::duplex(1024);
        let server = async {
            handshake(&mut peer, 65535).await;
            receive(&mut peer).await;
            let bytes = [
                response_head(true),
                frame(6, 0, 0, b"12345678"),
                frame(7, 0, 0, &[0, 0, 0, 1, 0, 0, 0, 0]),
            ]
            .concat();
            peer.write_all(&bytes).await.unwrap();
            close(&mut peer).await;
        };
        let (result, ()) = timeout(Duration::from_secs(2), async {
            tokio::join!(client(stream, b"", deadlines()), server)
        })
        .await
        .unwrap();
        assert!(result.unwrap().body.is_empty());
    }

    #[tokio::test]
    async fn malformed_partial_reset_and_trailing_responses_never_publish() {
        for response in [
            vec![],
            frame(3, 0, 1, &[0; 4]),
            [response_head(false), frame(0, 0, 1, b"partial")].concat(),
            [response_head(true), frame(1, 5, 1, &[])].concat(),
            [response_head(true), frame(0, 1, 1, &[])].concat(),
            [response_head(true), vec![0]].concat(),
            [response_head(false), frame(4, 1, 0, &[])].concat(),
            frame(7, 0, 0, &[0; 8]),
        ] {
            let (stream, mut peer) = tokio::io::duplex(1024);
            let server = async {
                handshake(&mut peer, 65535).await;
                receive(&mut peer).await;
                peer.write_all(&response).await.unwrap();
                peer.shutdown().await.unwrap();
                let mut discarded = Vec::new();
                let _ = peer.read_to_end(&mut discarded).await;
            };
            let (result, ()) = timeout(Duration::from_secs(2), async {
                tokio::join!(client(stream, b"", deadlines()), server)
            })
            .await
            .unwrap();
            assert!(result.unwrap_err().request_may_have_reached_author());
        }
    }

    #[tokio::test]
    async fn request_header_and_body_silence_have_distinct_bounded_failures() {
        for expected in [FiniteHttpFailure::Write, FiniteHttpFailure::Head, FiniteHttpFailure::Body]
        {
            let (stream, mut peer) = tokio::io::duplex(1024);
            let server = async {
                handshake(&mut peer, if expected == FiniteHttpFailure::Write { 0 } else { 65535 })
                    .await;
                receive(&mut peer).await;
                if expected == FiniteHttpFailure::Body {
                    peer.write_all(&response_head(false)).await.unwrap();
                }
                assert_eq!(peer.read(&mut [0; 1]).await.unwrap(), 0);
            };
            let mut limits = deadlines();
            limits.request_body_milliseconds = 30;
            limits.response_header_milliseconds = 30;
            limits.finite_idle_milliseconds = 30;
            let body = if expected == FiniteHttpFailure::Write { b"x".as_slice() } else { b"" };
            let (result, ()) = timeout(Duration::from_secs(2), async {
                tokio::join!(client(stream, body, limits), server)
            })
            .await
            .unwrap();
            assert_eq!(result.unwrap_err(), expected);
        }
    }

    #[tokio::test(start_paused = true)]
    async fn idle_budget_resets_on_each_byte_not_only_after_a_whole_frame() {
        let (input, mut output) = tokio::io::duplex(16);
        let mut input = IdleRead::new(input);
        input.enable(Duration::from_millis(10));
        let reader = async {
            let mut bytes = [0; 5];
            input.read_exact(&mut bytes).await.unwrap();
            assert_eq!(bytes, [1; 5]);
        };
        let writer = async {
            for _ in 0..5 {
                sleep(Duration::from_millis(9)).await;
                output.write_all(&[1]).await.unwrap();
            }
        };
        tokio::join!(reader, writer);
    }

    #[tokio::test(start_paused = true)]
    async fn end_stream_without_peer_eof_is_not_a_finite_receipt() {
        let (stream, mut peer) = tokio::io::duplex(1024);
        let mut limits = deadlines();
        limits.finite_idle_milliseconds = 10;
        let server = async {
            handshake(&mut peer, 65535).await;
            receive(&mut peer).await;
            peer.write_all(&response_head(true)).await.unwrap();
            assert_eq!(receive(&mut peer).await.kind, 7);
            assert_eq!(peer.read(&mut [0; 1]).await.unwrap(), 0);
            // Keep the peer's writing half open beyond the closure deadline.
            sleep(Duration::from_millis(20)).await;
        };
        let (result, ()) = tokio::join!(client(stream, b"", limits), server);
        assert_eq!(result.unwrap_err(), FiniteHttpFailure::Body);
    }

    #[tokio::test(start_paused = true)]
    async fn control_traffic_cannot_extend_the_finite_total_deadline() {
        let (stream, mut peer) = tokio::io::duplex(1024);
        let mut limits = deadlines();
        limits.finite_idle_milliseconds = 10;
        limits.finite_total_milliseconds = 30;
        let server = async {
            handshake(&mut peer, 65535).await;
            receive(&mut peer).await;
            peer.write_all(&response_head(false)).await.unwrap();
            loop {
                sleep(Duration::from_millis(5)).await;
                if peer.write_all(&frame(6, 0, 0, b"12345678")).await.is_err() {
                    break;
                }
                let mut ack = [0; 17];
                if peer.read_exact(&mut ack).await.is_err() {
                    break;
                }
            }
        };
        let started = Instant::now();
        let (result, ()) = tokio::join!(client(stream, b"", limits), server);
        assert_eq!(result.unwrap_err(), FiniteHttpFailure::Body);
        assert!(started.elapsed() >= Duration::from_millis(30));
        assert!(started.elapsed() < Duration::from_millis(60));
    }

    #[tokio::test]
    async fn caller_cancellation_drops_both_halves_without_background_sends() {
        let (stream, mut peer) = tokio::io::duplex(1024);
        let (observed, request_seen) = oneshot::channel();
        let cancelled = async {
            let future = client(stream, b"body awaiting flow credit", deadlines());
            tokio::pin!(future);
            tokio::select! {
                result = &mut future => panic!("unexpected completion: {result:?}"),
                _ = request_seen => {}
            }
            // Leaving this scope drops both joined I/O futures and the socket.
        };
        let server = async {
            handshake(&mut peer, 0).await;
            assert_eq!(receive(&mut peer).await.kind, 1);
            observed.send(()).unwrap();
            assert_eq!(peer.read(&mut [0; 1]).await.unwrap(), 0);
        };
        timeout(Duration::from_secs(2), async { tokio::join!(cancelled, server) }).await.unwrap();
    }

    // A deterministic consumer isolates the driver's deadline policy. Actual
    // event validity remains the event decoder's responsibility.
    struct LiveFixture {
        response: FiniteResponse,
        deadline: Option<Instant>,
    }
    impl ResponseConsumer for LiveFixture {
        type Output = SelectedAuthorFiniteResponse;
        fn head_complete(&self) -> bool {
            self.response.head_complete()
        }
        fn stream_ended(&self) -> bool {
            self.response.stream_ended()
        }
        fn accept(
            &mut self,
            frame: &ResponseFrame,
        ) -> Result<Option<[[u8; 13]; 2]>, crate::selected_author_http2_response::ResponseRefusal>
        {
            let credits = self.response.accept(frame)?;
            if self.deadline.is_none() && self.head_complete()
                || frame.kind == 0 && frame.payload == b":\n"
            {
                self.deadline = Some(Instant::now() + Duration::from_millis(10));
            }
            Ok(credits)
        }
        fn finish_at_transport_end(
            self,
            end: crate::selected_author_http2_frames::TransportEnd,
        ) -> Result<Self::Output, crate::selected_author_http2_response::ResponseRefusal> {
            self.response.finish_at_transport_end(end)
        }
        fn liveness_deadline(&self) -> Option<Instant> {
            self.deadline
        }
    }

    #[tokio::test(start_paused = true)]
    async fn live_consumer_heartbeats_outlive_finite_limits_but_ping_traffic_does_not() {
        for heartbeat in [true, false] {
            let (mut stream, mut peer) = tokio::io::duplex(1024);
            let request = async {
                let negotiated = crate::selected_author_http2_handshake::negotiate(
                    &mut stream,
                    Duration::from_secs(1),
                )
                .await
                .unwrap();
                let mut limits = deadlines();
                limits.finite_total_milliseconds = 1;
                limits.finite_idle_milliseconds = 1;
                drive_response(
                    stream,
                    negotiated,
                    [frame(1, 5, 1, &[0x82, 0x86, 0x84])].into_iter(),
                    b"",
                    limits,
                    LiveFixture { response: FiniteResponse::new(), deadline: None },
                )
                .await
            };
            let server = async {
                handshake(&mut peer, 65535).await;
                receive(&mut peer).await;
                peer.write_all(&response_head(false)).await.unwrap();
                for _ in 0..5 {
                    sleep(Duration::from_millis(5)).await;
                    let bytes = if heartbeat {
                        frame(0, 0, 1, b":\n")
                    } else {
                        frame(6, 0, 0, b"12345678")
                    };
                    if peer.write_all(&bytes).await.is_err() {
                        return;
                    }
                    let mut acknowledgement = vec![0; if heartbeat { 26 } else { 17 }];
                    if peer.read_exact(&mut acknowledgement).await.is_err() {
                        return;
                    }
                }
                assert!(heartbeat);
                peer.write_all(&frame(0, 1, 1, &[])).await.unwrap();
                close(&mut peer).await;
            };
            let (result, ()) = tokio::join!(request, server);
            assert_eq!(result.is_ok(), heartbeat);
            if !heartbeat {
                assert_eq!(result.unwrap_err(), FiniteHttpFailure::EventHeartbeat);
            }
        }
    }
}
