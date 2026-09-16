//! Driver fixtures for HTTP/2 flow control, completion and cancellation.

use super::*;
use tokio::io::{AsyncReadExt, DuplexStream};
use tokio::time::timeout;

// Fixture values remain independent of the production encoder constants.
const FRAME_HEADER_BYTES: usize = 9;
const INITIAL_HANDSHAKE_BYTES: usize = 39;
const DUPLEX_CAPACITY: usize = 1024;
const INITIAL_STREAM_WINDOW: u32 = 65_535;
const SETTINGS_KIND: u8 = 4;
const PING_KIND: u8 = 6;
const GOAWAY_KIND: u8 = 7;
const WINDOW_UPDATE_KIND: u8 = 8;
const END_HEADERS_FLAG: u8 = 4;
const END_HEADERS_AND_STREAM_FLAGS: u8 = 5;
const PHASE_BUDGET_MILLISECONDS: u64 = 1000;
const TOTAL_BUDGET_MILLISECONDS: u64 = 3000;
const SHORT_PHASE_MILLISECONDS: u64 = 30;
const IDLE_BUDGET_MILLISECONDS: u64 = 10;
const CONTROL_INTERVAL_MILLISECONDS: u64 = 5;
const TEST_TIMEOUT_SECONDS: u64 = 2;
const LONG_TEST_TIMEOUT_SECONDS: u64 = 5;
const LENGTH_HIGH_BYTE_SHIFT: u32 = 16;
const TINY_STREAM_WINDOW: u32 = 7;
const REPLENISHED_WINDOW: u32 = 100_000;
const RESPONSE_CHUNKS: usize = 10;
const BYTEWISE_DUPLEX_CAPACITY: usize = 16;
const BYTEWISE_READ_LENGTH: usize = 5;
const BYTE_INTERVAL_MILLISECONDS: u64 = 9;
const PEER_LINGER_MILLISECONDS: u64 = 20;
const PING_ACKNOWLEDGEMENT_BYTES: usize = 17;
const HEARTBEAT_COUNT: usize = 5;

pub(crate) fn frame(kind: u8, flags: u8, stream: u32, payload: &[u8]) -> Vec<u8> {
    let length = (payload.len() as u32).to_be_bytes();
    let mut result = vec![length[1], length[2], length[3], kind, flags];
    result.extend_from_slice(&stream.to_be_bytes());
    result.extend_from_slice(payload);
    result
}

pub(crate) async fn receive(peer: &mut DuplexStream) -> ResponseFrame {
    let mut header = [0; FRAME_HEADER_BYTES];
    peer.read_exact(&mut header).await.unwrap();
    let length = usize::from(header[0]) << LENGTH_HIGH_BYTE_SHIFT
        | usize::from(header[1]) << u8::BITS
        | usize::from(header[2]);
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
    let mut preface = [0; INITIAL_HANDSHAKE_BYTES];
    peer.read_exact(&mut preface).await.unwrap();
    assert_eq!(&preface[..24], b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n");
    let mut setting = vec![0, 4];
    setting.extend_from_slice(&window.to_be_bytes());
    peer.write_all(&frame(SETTINGS_KIND, 0, 0, &setting)).await.unwrap();
    let ack = receive(peer).await;
    assert_eq!((ack.kind, ack.flags), (4, 1));
    peer.write_all(&frame(SETTINGS_KIND, 1, 0, &[])).await.unwrap();
}

fn response_head(ended: bool) -> Vec<u8> {
    let mut block = vec![0x88, 0x0f, 16, 16]; // :status 200; literal static name 31 content-type
    block.extend_from_slice(b"application/json");
    frame(1, if ended { END_HEADERS_AND_STREAM_FLAGS } else { END_HEADERS_FLAG }, 1, &block)
}

pub(crate) fn deadlines() -> ExchangeDeadlines {
    ExchangeDeadlines {
        connect_milliseconds: PHASE_BUDGET_MILLISECONDS,
        transport_layer_security_milliseconds: PHASE_BUDGET_MILLISECONDS,
        request_body_milliseconds: PHASE_BUDGET_MILLISECONDS,
        response_header_milliseconds: PHASE_BUDGET_MILLISECONDS,
        finite_idle_milliseconds: PHASE_BUDGET_MILLISECONDS,
        finite_total_milliseconds: TOTAL_BUDGET_MILLISECONDS,
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
        [frame(
            1,
            if body.is_empty() { END_HEADERS_AND_STREAM_FLAGS } else { END_HEADERS_FLAG },
            1,
            &block,
        )]
        .into_iter(),
        body,
        deadlines,
    )
    .await
}

pub(crate) async fn close(peer: &mut DuplexStream) {
    loop {
        let outgoing = receive(peer).await;
        if outgoing.kind == GOAWAY_KIND {
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
    let (stream, mut peer) = tokio::io::duplex(DUPLEX_CAPACITY);
    let body = vec![b'x'; 100_000];
    let server = async {
        handshake(&mut peer, TINY_STREAM_WINDOW).await;
        assert_eq!(receive(&mut peer).await.kind, 1);
        let first = receive(&mut peer).await;
        assert_eq!(first.payload, vec![b'x'; 7]);
        // No stream credit remains: the client must continue reading.
        peer.write_all(&frame(PING_KIND, 0, 0, b"12345678")).await.unwrap();
        let ack = receive(&mut peer).await;
        assert_eq!((ack.kind, ack.flags, ack.payload.as_slice()), (6, 1, b"12345678".as_slice()));
        peer.write_all(&frame(WINDOW_UPDATE_KIND, 0, 1, &REPLENISHED_WINDOW.to_be_bytes()))
            .await
            .unwrap();
        peer.write_all(&frame(WINDOW_UPDATE_KIND, 0, 0, &REPLENISHED_WINDOW.to_be_bytes()))
            .await
            .unwrap();
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
        for index in 0..RESPONSE_CHUNKS {
            peer.write_all(&frame(
                0,
                u8::from(index == RESPONSE_CHUNKS - 1),
                1,
                &vec![b'y'; 10_000],
            ))
            .await
            .unwrap();
            if index != RESPONSE_CHUNKS - 1 {
                for stream in [0, 1] {
                    let credit = receive(&mut peer).await;
                    assert_eq!((credit.kind, credit.stream_identifier), (8, stream));
                    assert_eq!(credit.payload, 10_000u32.to_be_bytes());
                }
            }
        }
        close(&mut peer).await;
    };
    let (result, ()) = timeout(Duration::from_secs(LONG_TEST_TIMEOUT_SECONDS), async {
        tokio::join!(client(stream, &body, deadlines()), server)
    })
    .await
    .unwrap();
    assert_eq!(result.unwrap().body, vec![b'y'; 100_000]);
}

#[tokio::test]
async fn complete_early_response_closes_the_unfinished_request_without_retry() {
    let (stream, mut peer) = tokio::io::duplex(DUPLEX_CAPACITY);
    let server = async {
        handshake(&mut peer, 0).await;
        assert_eq!(receive(&mut peer).await.kind, 1);
        peer.write_all(&response_head(true)).await.unwrap();
        let reset = receive(&mut peer).await;
        assert_eq!((reset.kind, reset.stream_identifier), (3, 1));
        assert_eq!(reset.payload, [0; 4]);
        close(&mut peer).await;
    };
    let (result, ()) = timeout(Duration::from_secs(TEST_TIMEOUT_SECONDS), async {
        tokio::join!(client(stream, b"not sent", deadlines()), server)
    })
    .await
    .unwrap();
    assert!(result.unwrap().body.is_empty());
}

#[tokio::test]
async fn post_end_control_frames_do_not_issue_multiple_close_commands() {
    let (stream, mut peer) = tokio::io::duplex(DUPLEX_CAPACITY);
    let server = async {
        handshake(&mut peer, INITIAL_STREAM_WINDOW).await;
        receive(&mut peer).await;
        let bytes = [
            response_head(true),
            frame(PING_KIND, 0, 0, b"12345678"),
            frame(7, 0, 0, &[0, 0, 0, 1, 0, 0, 0, 0]),
        ]
        .concat();
        peer.write_all(&bytes).await.unwrap();
        close(&mut peer).await;
    };
    let (result, ()) = timeout(Duration::from_secs(TEST_TIMEOUT_SECONDS), async {
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
        [response_head(true), frame(1, END_HEADERS_AND_STREAM_FLAGS, 1, &[])].concat(),
        [response_head(true), frame(0, 1, 1, &[])].concat(),
        [response_head(true), vec![0]].concat(),
        [response_head(false), frame(SETTINGS_KIND, 1, 0, &[])].concat(),
        frame(7, 0, 0, &[0; 8]),
    ] {
        let (stream, mut peer) = tokio::io::duplex(DUPLEX_CAPACITY);
        let server = async {
            handshake(&mut peer, INITIAL_STREAM_WINDOW).await;
            receive(&mut peer).await;
            peer.write_all(&response).await.unwrap();
            peer.shutdown().await.unwrap();
            let mut discarded = Vec::new();
            let _ = peer.read_to_end(&mut discarded).await;
        };
        let (result, ()) = timeout(Duration::from_secs(TEST_TIMEOUT_SECONDS), async {
            tokio::join!(client(stream, b"", deadlines()), server)
        })
        .await
        .unwrap();
        assert!(result.unwrap_err().request_may_have_reached_author());
    }
}

#[tokio::test]
async fn request_header_and_body_silence_have_distinct_bounded_failures() {
    for expected in [FiniteHttpFailure::Write, FiniteHttpFailure::Head, FiniteHttpFailure::Body] {
        let (stream, mut peer) = tokio::io::duplex(DUPLEX_CAPACITY);
        let server = async {
            handshake(
                &mut peer,
                if expected == FiniteHttpFailure::Write { 0 } else { INITIAL_STREAM_WINDOW },
            )
            .await;
            receive(&mut peer).await;
            if expected == FiniteHttpFailure::Body {
                peer.write_all(&response_head(false)).await.unwrap();
            }
            assert_eq!(peer.read(&mut [0; 1]).await.unwrap(), 0);
        };
        let mut limits = deadlines();
        limits.request_body_milliseconds = SHORT_PHASE_MILLISECONDS;
        limits.response_header_milliseconds = SHORT_PHASE_MILLISECONDS;
        limits.finite_idle_milliseconds = SHORT_PHASE_MILLISECONDS;
        let body = if expected == FiniteHttpFailure::Write { b"x".as_slice() } else { b"" };
        let (result, ()) = timeout(Duration::from_secs(TEST_TIMEOUT_SECONDS), async {
            tokio::join!(client(stream, body, limits), server)
        })
        .await
        .unwrap();
        assert_eq!(result.unwrap_err(), expected);
    }
}

#[tokio::test(start_paused = true)]
async fn idle_budget_resets_on_each_byte_not_only_after_a_whole_frame() {
    let (input, mut output) = tokio::io::duplex(BYTEWISE_DUPLEX_CAPACITY);
    let mut input = IdleRead::new(input);
    input.enable(Duration::from_millis(IDLE_BUDGET_MILLISECONDS));
    let reader = async {
        let mut bytes = [0; BYTEWISE_READ_LENGTH];
        input.read_exact(&mut bytes).await.unwrap();
        assert_eq!(bytes, [1; 5]);
    };
    let writer = async {
        for _ in 0..BYTEWISE_READ_LENGTH {
            sleep(Duration::from_millis(BYTE_INTERVAL_MILLISECONDS)).await;
            output.write_all(&[1]).await.unwrap();
        }
    };
    tokio::join!(reader, writer);
}

#[tokio::test(start_paused = true)]
async fn end_stream_without_peer_eof_completes_finite_receipt() {
    let (stream, mut peer) = tokio::io::duplex(DUPLEX_CAPACITY);
    let mut limits = deadlines();
    limits.finite_idle_milliseconds = IDLE_BUDGET_MILLISECONDS;
    let server = async {
        handshake(&mut peer, INITIAL_STREAM_WINDOW).await;
        receive(&mut peer).await;
        peer.write_all(&response_head(true)).await.unwrap();
        assert_eq!(receive(&mut peer).await.kind, 7);
        assert_eq!(peer.read(&mut [0; 1]).await.unwrap(), 0);
        // Keep the peer's writing half open beyond the closure deadline.
        sleep(Duration::from_millis(PEER_LINGER_MILLISECONDS)).await;
    };
    let (result, ()) = tokio::join!(client(stream, b"", limits), server);
    assert!(result.is_ok());
}

#[tokio::test(start_paused = true)]
async fn control_traffic_cannot_extend_the_finite_total_deadline() {
    let (stream, mut peer) = tokio::io::duplex(DUPLEX_CAPACITY);
    let mut limits = deadlines();
    limits.finite_idle_milliseconds = IDLE_BUDGET_MILLISECONDS;
    limits.finite_total_milliseconds = SHORT_PHASE_MILLISECONDS;
    let server = async {
        handshake(&mut peer, INITIAL_STREAM_WINDOW).await;
        receive(&mut peer).await;
        peer.write_all(&response_head(false)).await.unwrap();
        loop {
            sleep(Duration::from_millis(CONTROL_INTERVAL_MILLISECONDS)).await;
            if peer.write_all(&frame(PING_KIND, 0, 0, b"12345678")).await.is_err() {
                break;
            }
            let mut ack = [0; PING_ACKNOWLEDGEMENT_BYTES];
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
    let (stream, mut peer) = tokio::io::duplex(DUPLEX_CAPACITY);
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
    timeout(Duration::from_secs(TEST_TIMEOUT_SECONDS), async { tokio::join!(cancelled, server) })
        .await
        .unwrap();
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
    ) -> Result<Option<FlowCredits>, crate::selected_author_http2_response::ResponseRefusal> {
        let credits = self.response.accept(frame)?;
        if self.deadline.is_none() && self.head_complete()
            || frame.kind == 0 && frame.payload == b":\n"
        {
            self.deadline = Some(Instant::now() + Duration::from_millis(IDLE_BUDGET_MILLISECONDS));
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
        let (mut stream, mut peer) = tokio::io::duplex(DUPLEX_CAPACITY);
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
                [frame(1, END_HEADERS_AND_STREAM_FLAGS, 1, &[0x82, 0x86, 0x84])].into_iter(),
                b"",
                limits,
                LiveFixture { response: FiniteResponse::new(), deadline: None },
            )
            .await
        };
        let server = async {
            handshake(&mut peer, INITIAL_STREAM_WINDOW).await;
            receive(&mut peer).await;
            peer.write_all(&response_head(false)).await.unwrap();
            for _ in 0..HEARTBEAT_COUNT {
                sleep(Duration::from_millis(CONTROL_INTERVAL_MILLISECONDS)).await;
                let bytes = if heartbeat {
                    frame(0, 0, 1, b":\n")
                } else {
                    frame(PING_KIND, 0, 0, b"12345678")
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
