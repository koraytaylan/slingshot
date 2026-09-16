//! Cancellation-owned IMS HTTP/2 request/control/response driver.

use super::{
    async_identity_management_exchange::IdentityManagementReceipt,
    identity_management_exchange::{
        DecodedResponse, ExchangeFailure, MonotonicClock, identity_management_endpoint,
    },
    identity_management_http2_response::IdentityManagementHttp2Response,
};
use crate::{
    selected_author_http2_flow::SendWindows,
    selected_author_http2_frames::{FrameRead, ResponseFrame, ResponseFrameReader},
    selected_author_http2_handshake::negotiate_frames,
};
use slingshot_domain::profile_authentication_contract::{
    ConfigurationFailureCode as Code, ProfileAuthenticationContract,
};
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::{
    io::{AsyncRead, AsyncWrite, AsyncWriteExt, ReadBuf},
    sync::{mpsc, oneshot, watch},
    time::{Duration, Instant, Sleep, sleep, sleep_until, timeout_at},
};

const ACK: &[u8] = &[0, 0, 0, 4, 1, 0, 0, 0, 0];
const RESET: &[u8] = &[0, 0, 4, 3, 0, 0, 0, 0, 1, 0, 0, 0, 0];
const GOAWAY: &[u8] = &[0, 0, 8, 7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
const DATA_FRAME_KIND: u8 = 0;
const HEADERS_FRAME_KIND: u8 = 1;
const RESET_FRAME_KIND: u8 = 3;
const SETTINGS_FRAME_KIND: u8 = 4;
const PING_FRAME_KIND: u8 = 6;
const GOAWAY_FRAME_KIND: u8 = 7;
const WINDOW_UPDATE_FRAME_KIND: u8 = 8;
const CONTINUATION_FRAME_KIND: u8 = 9;
const ACKNOWLEDGEMENT_FLAG: u8 = 1;
const STREAM_IDENTIFIER_MASK: u32 = 0x7fff_ffff;
const STATIC_NAME_PREFIX_MAXIMUM: u8 = 15;
const NEVER_INDEXED_PREFIX: u8 = 0x10;
const STRING_LENGTH_PREFIX_MAXIMUM: usize = 127;
const PING_FRAME_BYTES: usize = 17;
enum Command {
    Control(ResponseFrame),
    Credit([[u8; 13]; 2], oneshot::Sender<()>),
    Finish,
}

/// Negotiates and sends exactly one fixed IMS POST on this authenticated socket.
/// No task is spawned. The caller also applies the whole-exchange deadline,
/// including DNS/TCP/TLS setup, around this future.
///
/// # Errors
/// Refuses oversized requests, invalid framing or responses, failed transport
/// operations, and expired write, header, idle-body or total-body deadlines.
pub async fn exchange_http2<Stream: AsyncRead + AsyncWrite + Unpin>(
    mut stream: Stream,
    body: &[u8],
    clock: &(dyn MonotonicClock + Sync),
) -> Result<IdentityManagementReceipt, ExchangeFailure> {
    let limits = &ProfileAuthenticationContract::embedded().limits;
    if body.len() as u64 > limits.maximum_identity_management_request_body_bytes {
        return Err(fail(Code::IdentityManagementResponseHeadLimitExceeded));
    }
    let head = request_head(body.len())?;
    let deadline = Instant::now()
        + Duration::from_millis(limits.identity_management_response_header_timeout_milliseconds);
    let negotiated = timeout_at(
        deadline,
        negotiate_frames(
            &mut stream,
            ResponseFrameReader::with_header_policy(
                limits.maximum_identity_management_response_head_bytes,
                true,
            ),
        ),
    )
    .await
    .map_err(|_| fail(Code::IdentityManagementResponseHeaderTimeout))?
    .map_err(|_| malformed())?;
    if Instant::now() >= deadline {
        return Err(fail(Code::IdentityManagementResponseHeaderTimeout));
    }
    let (input, output) = tokio::io::split(stream);
    let (commands, pending) = mpsc::channel(1);
    let (completed, completion) = watch::channel(None);
    let request_end = Instant::now()
        + Duration::from_millis(limits.identity_management_request_write_timeout_milliseconds);
    let writer =
        write(output, negotiated.send_windows, &head, body, pending, completed, request_end, clock);
    let reader = read(input, negotiated.frames, commands, completion, request_end, clock);
    let (anchor, (response, receipt)) = tokio::try_join!(writer, reader)?;
    Ok(IdentityManagementReceipt::new(response, anchor, receipt))
}

fn request_head(length: usize) -> Result<Vec<u8>, ExchangeFailure> {
    let endpoint = url::Url::parse(&identity_management_endpoint()).map_err(|_| malformed())?;
    let mut block = vec![0x83, 0x87]; // :method POST, :scheme https
    for (index, value) in [
        (1, endpoint.host_str().ok_or_else(malformed)?),
        (4, endpoint.path()),
        (31, "application/x-www-form-urlencoded"),
        (19, "application/json"),
        (28, &length.to_string()),
    ] {
        // Literal never indexed, using the static name. No credential is a field.
        if index < STATIC_NAME_PREFIX_MAXIMUM {
            block.push(NEVER_INDEXED_PREFIX | index);
        } else {
            block.extend_from_slice(&[0x1f, index - STATIC_NAME_PREFIX_MAXIMUM]);
        }
        if value.len() >= STRING_LENGTH_PREFIX_MAXIMUM {
            return Err(malformed());
        }
        block.push(value.len() as u8);
        block.extend_from_slice(value.as_bytes());
    }
    let size = (block.len() as u32).to_be_bytes();
    let mut frame = vec![size[1], size[2], size[3], 1, 4 | u8::from(length == 0), 0, 0, 0, 1];
    frame.extend_from_slice(&block);
    Ok(frame)
}

async fn write<Writer: AsyncWrite + Unpin>(
    mut output: Writer,
    mut windows: SendWindows,
    head: &[u8],
    body: &[u8],
    mut commands: mpsc::Receiver<Command>,
    completed: watch::Sender<Option<Instant>>,
    end: Instant,
    clock: &(dyn MonotonicClock + Sync),
) -> Result<u64, ExchangeFailure> {
    let mut position = 0;
    let anchor = clock.reading_milliseconds();
    let early = timeout_at(end, async {
        send(&mut output, head).await?;
        while position < body.len() {
            match commands.try_recv() {
                Ok(Command::Finish) => return Ok(true),
                Ok(command) => {
                    control(&mut output, &mut windows, command, false).await?;
                    continue;
                }
                Err(mpsc::error::TryRecvError::Disconnected) => return Err(malformed()),
                Err(mpsc::error::TryRecvError::Empty) => {}
            }
            let count = windows.reserve(body.len() - position);
            if count == 0 {
                match commands.recv().await.ok_or_else(malformed)? {
                    Command::Finish => return Ok(true),
                    command => control(&mut output, &mut windows, command, false).await?,
                }
                continue;
            }
            let size = (count as u32).to_be_bytes();
            let header = [
                size[1],
                size[2],
                size[3],
                0,
                u8::from(position + count == body.len()),
                0,
                0,
                0,
                1,
            ];
            output.write_all(&header).await.map_err(|_| malformed())?;
            send(&mut output, &body[position..position + count]).await?;
            position += count;
        }
        Ok(false)
    })
    .await
    .map_err(|_| fail(Code::IdentityManagementRequestWriteTimeout))??;
    if Instant::now() >= end {
        return Err(fail(Code::IdentityManagementRequestWriteTimeout));
    }
    if !early {
        completed.send_replace(Some(Instant::now()));
        finish_controls(&mut output, &mut windows, &mut commands).await?;
    }
    if position < body.len() {
        send(&mut output, RESET).await?;
    }
    send(&mut output, GOAWAY).await?;
    output.shutdown().await.map_err(|_| malformed())?;
    Ok(anchor)
}
async fn finish_controls(
    output: &mut (impl AsyncWrite + Unpin),
    windows: &mut SendWindows,
    commands: &mut mpsc::Receiver<Command>,
) -> Result<(), ExchangeFailure> {
    loop {
        match commands.recv().await.ok_or_else(malformed)? {
            Command::Finish => return Ok(()),
            command => control(output, windows, command, true).await?,
        }
    }
}

async fn send(output: &mut (impl AsyncWrite + Unpin), bytes: &[u8]) -> Result<(), ExchangeFailure> {
    output.write_all(bytes).await.map_err(|_| malformed())?;
    output.flush().await.map_err(|_| malformed())
}
async fn control(
    output: &mut (impl AsyncWrite + Unpin),
    windows: &mut SendWindows,
    command: Command,
    complete: bool,
) -> Result<(), ExchangeFailure> {
    match command {
        Command::Control(frame) => match frame.kind {
            SETTINGS_FRAME_KIND => {
                windows.observe(&frame).map_err(|_| malformed())?;
                send(output, ACK).await?;
            }
            WINDOW_UPDATE_FRAME_KIND if complete && frame.stream_identifier == 1 => {}
            WINDOW_UPDATE_FRAME_KIND => windows.observe(&frame).map_err(|_| malformed())?,
            PING_FRAME_KIND => {
                let mut ack = [0; PING_FRAME_BYTES];
                ack[..9].copy_from_slice(&[0, 0, 8, 6, 1, 0, 0, 0, 0]);
                ack[9..].copy_from_slice(&frame.payload);
                send(output, &ack).await?;
            }
            _ => return Err(malformed()),
        },
        Command::Credit(frames, written) => {
            for frame in frames {
                send(output, &frame).await?;
            }
            let _ = written.send(());
        }
        Command::Finish => return Err(malformed()),
    }
    Ok(())
}

async fn read<Reader: AsyncRead + Unpin>(
    input: Reader,
    mut frames: ResponseFrameReader,
    commands: mpsc::Sender<Command>,
    mut completion: watch::Receiver<Option<Instant>>,
    request_end: Instant,
    clock: &(dyn MonotonicClock + Sync),
) -> Result<(DecodedResponse, u64), ExchangeFailure> {
    let limits = &ProfileAuthenticationContract::embedded().limits;
    let mut input = IdleRead::new(input);
    let mut response = IdentityManagementHttp2Response::new();
    let mut body_end = None;
    loop {
        let frame =
            read_next_frame(&mut input, &mut frames, &mut completion, request_end, body_end)
                .await?;
        match frame {
            FrameRead::End(end) => {
                let receipt = clock.reading_milliseconds();
                return Ok((response.finish_at_transport_end(end)?, receipt));
            }
            FrameRead::Frame(frame) => {
                let pending = frame_command(frame, &mut response)?;
                if response.head_complete() && body_end.is_none() {
                    body_end = Some(
                        Instant::now()
                            + Duration::from_millis(
                                limits.identity_management_response_body_total_timeout_milliseconds,
                            ),
                    );
                    input.enable(Duration::from_millis(
                        limits.identity_management_response_body_idle_timeout_milliseconds,
                    ));
                }
                let end = body_end.unwrap_or_else(|| {
                    (*completion.borrow()).unwrap_or(request_end)
                        + Duration::from_millis(
                            limits.identity_management_response_header_timeout_milliseconds,
                        )
                });
                let end = input.idle_end().map_or(end, |idle| end.min(idle));
                let expired = if body_end.is_none() {
                    Code::IdentityManagementResponseHeaderTimeout
                } else if body_end.is_some_and(|total| total <= end) {
                    Code::IdentityManagementResponseBodyTotalTimeout
                } else {
                    Code::IdentityManagementResponseBodyIdleTimeout
                };
                if Instant::now() >= end {
                    return Err(fail(expired));
                }
                dispatch_frame_command(&commands, pending, response.stream_ended(), end, expired)
                    .await?;
                if response.stream_ended() {
                    timeout_at(end, commands.send(Command::Finish))
                        .await
                        .map_err(|_| fail(expired))?
                        .map_err(|_| malformed())?;
                    return Ok((response.finish_at_stream_end()?, clock.reading_milliseconds()));
                }
            }
        }
    }
}

async fn dispatch_frame_command(
    commands: &mpsc::Sender<Command>,
    pending: FrameCommand,
    stream_ended: bool,
    end: Instant,
    expired: Code,
) -> Result<(), ExchangeFailure> {
    if let Some(command) = pending.command {
        if !stream_ended {
            timeout_at(end, commands.send(command))
                .await
                .map_err(|_| fail(expired))?
                .map_err(|_| malformed())?;
        }
    }
    if let Some(written) = pending.confirmation {
        timeout_at(end, written).await.map_err(|_| fail(expired))?.map_err(|_| malformed())?;
    }
    Ok(())
}

// Keep the same read future pinned while request completion changes the
// header deadline: restarting it could discard a partially consumed frame.
async fn read_next_frame<Reader: AsyncRead + Unpin>(
    input: &mut IdleRead<Reader>,
    frames: &mut ResponseFrameReader,
    completion: &mut watch::Receiver<Option<Instant>>,
    request_end: Instant,
    body_end: Option<Instant>,
) -> Result<FrameRead, ExchangeFailure> {
    let limits = &ProfileAuthenticationContract::embedded().limits;
    let result = {
        let next = frames.read_next(input);
        tokio::pin!(next);
        loop {
            let complete = *completion.borrow();
            let end = body_end.unwrap_or_else(|| {
                complete.unwrap_or(request_end)
                    + Duration::from_millis(
                        limits.identity_management_response_header_timeout_milliseconds,
                    )
            });
            let expired = if body_end.is_some() {
                Code::IdentityManagementResponseBodyTotalTimeout
            } else {
                Code::IdentityManagementResponseHeaderTimeout
            };
            if Instant::now() >= end {
                return Err(fail(expired));
            }
            tokio::select! {
                result = &mut next => { if Instant::now() >= end { return Err(fail(expired)); } break result; },
                changed = completion.changed(), if body_end.is_none() && complete.is_none() => { changed.map_err(|_| malformed())?; },
                _ = sleep_until(end) => return Err(fail(expired)),
            }
        }
    };
    result.map_err(|_| {
        fail(if input.expired {
            Code::IdentityManagementResponseBodyIdleTimeout
        } else if frames.header_limit_exceeded() {
            Code::IdentityManagementResponseHeadLimitExceeded
        } else {
            Code::IdentityManagementTransportFailed
        })
    })
}

struct FrameCommand {
    command: Option<Command>,
    confirmation: Option<oneshot::Receiver<()>>,
}

fn frame_command(
    frame: ResponseFrame,
    response: &mut IdentityManagementHttp2Response,
) -> Result<FrameCommand, ExchangeFailure> {
    let mut confirmation = None;
    let command = match frame.kind {
        DATA_FRAME_KIND | HEADERS_FRAME_KIND | CONTINUATION_FRAME_KIND => {
            let credits = response.accept(&frame)?;
            if response.stream_ended() {
                None
            } else {
                credits.map(|frames| {
                    let (written, pending) = oneshot::channel();
                    confirmation = Some(pending);
                    Command::Credit(frames, written)
                })
            }
        }
        RESET_FRAME_KIND => return Err(malformed()),
        SETTINGS_FRAME_KIND if frame.flags & ACKNOWLEDGEMENT_FLAG != 0 => return Err(malformed()),
        SETTINGS_FRAME_KIND | WINDOW_UPDATE_FRAME_KIND => Some(Command::Control(frame)),
        PING_FRAME_KIND if frame.flags & ACKNOWLEDGEMENT_FLAG == 0 => Some(Command::Control(frame)),
        GOAWAY_FRAME_KIND => {
            require_successful_goaway(&frame)?;
            None
        }
        _ => None,
    };
    Ok(FrameCommand { command, confirmation })
}

fn require_successful_goaway(frame: &ResponseFrame) -> Result<(), ExchangeFailure> {
    let last = u32::from_be_bytes(frame.payload[..4].try_into().unwrap()) & STREAM_IDENTIFIER_MASK;
    let error = u32::from_be_bytes(frame.payload[4..8].try_into().unwrap());
    if last < 1 || error != 0 {
        return Err(malformed());
    }
    Ok(())
}

struct IdleRead<Reader> {
    input: Reader,
    duration: Option<Duration>,
    timer: Pin<Box<Sleep>>,
    expired: bool,
}
impl<Reader> IdleRead<Reader> {
    fn new(input: Reader) -> Self {
        Self { input, duration: None, timer: Box::pin(sleep(Duration::ZERO)), expired: false }
    }
    fn enable(&mut self, duration: Duration) {
        self.duration = Some(duration);
        self.timer.as_mut().reset(Instant::now() + duration);
    }
    fn idle_end(&self) -> Option<Instant> {
        self.duration.map(|_| self.timer.deadline())
    }
}
impl<Reader: AsyncRead + Unpin> AsyncRead for IdleRead<Reader> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        if self.duration.is_some() && self.timer.as_mut().poll(context).is_ready() {
            self.expired = true;
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
fn fail(code: Code) -> ExchangeFailure {
    ExchangeFailure::new(code)
}
fn malformed() -> ExchangeFailure {
    fail(Code::IdentityManagementTransportFailed)
}

#[cfg(test)]
#[path = "identity_management_http2_tests.rs"]
mod tests;
