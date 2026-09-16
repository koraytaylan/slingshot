//! HTTP/1.1 event attachment through shared selected-author/event policies.
//! One explicit fixed-length or chunked body; no retry or partial cursor fact.

use crate::authentication::environment_provider::RequestAuthentication;
use crate::author_hypertext_transfer_protocol_policy::ExchangeDeadlines;
use crate::selected_author_events::{
    EventDelivery, EventHttpOutcome, classify_attachment, request_fields,
};
use crate::selected_author_exchange::{
    CollectedFiniteResponse, validate_collected_finite_response, validate_finite_head,
};
use crate::selected_author_http::{
    BodyFraming, FiniteHttpFailure, decode_chunk_size, encode_request, read_chunk_line,
    read_framed_body, read_head,
};
use crate::selected_author_transport::SelectedAuthorTransport;
use crate::server_sent_event_decoder::{
    EventStreamCursor, StreamItem, TerminalExpectationResolver,
};
use http::{Method, Response, StatusCode, Version};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::time::{Duration, Instant, timeout, timeout_at};

const STREAM_READ_BUFFER_BYTES: usize = 8192;

impl SelectedAuthorTransport {
    /// Attaches once to the selected subscription and committed cursor. The
    /// caller owns retained terminal resolution, durable folding and reconnects.
    ///
    /// # Errors
    /// Refuses invalid selection, cursor or request fields; connection/write
    /// failures; malformed, truncated or overdue responses; invalid reset
    /// evidence; and consumer refusals. Previously delivered items are not retracted.
    pub async fn events_http1<Resolver: TerminalExpectationResolver>(
        &self,
        identity: &slingshot_domain::operation_executor::ExecutionIdentity,
        subscription: &str,
        generation: u64,
        committed_cursor: Option<&EventStreamCursor>,
        authentication: &RequestAuthentication,
        resolver: Resolver,
        consume: impl FnMut(StreamItem) -> Result<(), FiniteHttpFailure>,
    ) -> Result<EventHttpOutcome, FiniteHttpFailure> {
        let (fields, operation) =
            request_fields(self, identity, subscription, generation, committed_cursor)?;
        let generation_text = generation.to_string();
        let request = encode_request(
            self,
            Method::GET,
            &["bin", "slingshot", "agent", "events"],
            &[
                ("agent_event_store_generation", &generation_text),
                ("agent_operation_identifier", &operation),
                ("daemon_subscription_identifier", subscription),
            ],
            authentication,
            &fields,
            b"",
        )?;
        let stream = self.connect().await.map_err(|_| FiniteHttpFailure::Connect)?;
        Self::events_http1_on_stream(
            stream,
            &request,
            subscription,
            generation,
            committed_cursor,
            resolver,
            consume,
        )
        .await
    }

    pub(crate) async fn events_http1_on_stream<Resolver: TerminalExpectationResolver>(
        mut stream: crate::selected_author_transport::SelectedAuthorStream,
        request: &[u8],
        subscription: &str,
        generation: u64,
        committed_cursor: Option<&EventStreamCursor>,
        resolver: Resolver,
        consume: impl FnMut(StreamItem) -> Result<(), FiniteHttpFailure>,
    ) -> Result<EventHttpOutcome, FiniteHttpFailure> {
        let deadlines = ExchangeDeadlines::embedded();
        timeout(Duration::from_millis(deadlines.request_body_milliseconds), async {
            stream.write_all(request).await?;
            stream.flush().await
        })
        .await
        .map_err(|_| FiniteHttpFailure::Write)?
        .map_err(|_| FiniteHttpFailure::Write)?;
        receive(&mut stream, subscription.to_owned(), generation, resolver, consume, deadlines)
            .await
            .and_then(|outcome| {
                classify_attachment(outcome, subscription, generation, committed_cursor)
            })
    }
}

async fn receive<Resolver: TerminalExpectationResolver>(
    stream: &mut (impl AsyncRead + Unpin),
    subscription: String,
    generation: u64,
    resolver: Resolver,
    consume: impl FnMut(StreamItem) -> Result<(), FiniteHttpFailure>,
    deadlines: ExchangeDeadlines,
) -> Result<EventHttpOutcome, FiniteHttpFailure> {
    let (status, headers, framing) =
        timeout(Duration::from_millis(deadlines.response_header_milliseconds), read_head(stream))
            .await
            .map_err(|_| FiniteHttpFailure::Head)??;
    let status = StatusCode::from_u16(status).map_err(|_| FiniteHttpFailure::Head)?;
    let (head, media) = validate_finite_head(status, Version::HTTP_11, &headers)
        .map_err(|_| FiniteHttpFailure::Head)?;
    if head.location.is_some() {
        return Err(FiniteHttpFailure::Head);
    }
    if status != StatusCode::OK {
        if !crate::selected_author_submission::json_media_type(&media) {
            return Err(FiniteHttpFailure::Head);
        }
        let limit = slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract::embedded().limit("maximum_finite_response_body_bytes");
        let body = timeout(
            Duration::from_millis(deadlines.finite_total_milliseconds),
            read_framed_body(
                stream,
                framing,
                limit,
                Duration::from_millis(deadlines.finite_idle_milliseconds),
            ),
        )
        .await
        .map_err(|_| FiniteHttpFailure::Body)??;
        let mut response = Response::new(body);
        *response.status_mut() = status;
        *response.version_mut() = Version::HTTP_11;
        *response.headers_mut() = headers;
        return validate_collected_finite_response(CollectedFiniteResponse {
            response,
            framing_ambiguous: false,
            trailer_section_present: false,
            trailing_bytes: false,
        })
        .map(EventHttpOutcome::Response)
        .map_err(|_| FiniteHttpFailure::Body);
    }
    let mut delivery =
        EventDelivery::attached(&head, &media, subscription, generation, resolver, consume)?;
    match framing {
        BodyFraming::Fixed(length) => part(stream, length, &mut delivery).await?,
        BodyFraming::Chunked => loop {
            let line = chunk_line(stream, delivery.deadline()).await?;
            let length = decode_chunk_size(&line)?;
            if length == 0 {
                if !chunk_line(stream, delivery.deadline()).await?.is_empty() {
                    return Err(FiniteHttpFailure::Body);
                }
                break;
            }
            part(stream, length, &mut delivery).await?;
            if !chunk_line(stream, delivery.deadline()).await?.is_empty() {
                return Err(FiniteHttpFailure::Body);
            }
        },
    }
    let mut extra = [0; 1];
    if within(delivery.deadline(), async {
        stream.read(&mut extra).await.map_err(|_| FiniteHttpFailure::Body)
    })
    .await?
        != 0
    {
        return Err(FiniteHttpFailure::Body);
    }
    delivery.finish()?;
    Ok(EventHttpOutcome::Closed)
}

async fn part<
    Resolver: TerminalExpectationResolver,
    Consumer: FnMut(StreamItem) -> Result<(), FiniteHttpFailure>,
>(
    stream: &mut (impl AsyncRead + Unpin),
    mut remaining: u64,
    delivery: &mut EventDelivery<Resolver, Consumer>,
) -> Result<(), FiniteHttpFailure> {
    let mut bytes = [0; STREAM_READ_BUFFER_BYTES];
    while remaining != 0 {
        let count = remaining.min(bytes.len() as u64) as usize;
        let count = within(delivery.deadline(), async {
            stream.read(&mut bytes[..count]).await.map_err(|_| FiniteHttpFailure::Body)
        })
        .await?;
        if count == 0 {
            return Err(FiniteHttpFailure::Body);
        }
        delivery.push(&bytes[..count])?;
        remaining -= count as u64;
    }
    Ok(())
}

async fn chunk_line(
    stream: &mut (impl AsyncRead + Unpin),
    deadline: Instant,
) -> Result<Vec<u8>, FiniteHttpFailure> {
    let idle = Duration::from_millis(
        crate::event_stream_heartbeat::heartbeat_timeout_milliseconds().saturating_add(1),
    );
    within(deadline, read_chunk_line(stream, idle)).await
}

async fn within<Output>(
    deadline: Instant,
    future: impl core::future::Future<Output = Result<Output, FiniteHttpFailure>>,
) -> Result<Output, FiniteHttpFailure> {
    if Instant::now() >= deadline {
        return Err(FiniteHttpFailure::EventHeartbeat);
    }
    let result =
        timeout_at(deadline, future).await.map_err(|_| FiniteHttpFailure::EventHeartbeat)?;
    if Instant::now() >= deadline {
        return Err(FiniteHttpFailure::EventHeartbeat);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::selected_author_http2::tests::deadlines;
    use crate::selected_author_http2_events::tests::{resolve, terminal};
    use tokio::time::sleep;

    const DUPLEX_CAPACITY_BYTES: usize = 4096;
    const TEST_GENERATION: u64 = 7;
    const HEARTBEAT_COUNT: usize = 3;
    const HEARTBEAT_INTERVAL_SECONDS: u64 = 20;
    const SHORT_FINITE_TIMEOUT_MILLISECONDS: u64 = 10;
    const ACTIVITY_WRITES: u64 = 5;
    const ACTIVITY_INTERVAL_SECONDS: u64 = 10;

    #[tokio::test(start_paused = true)]
    async fn fixed_and_chunked_live_streams_outlast_finite_limits_with_shared_terminal_validation()
    {
        for chunked in [false, true] {
            let (mut input, mut peer) = tokio::io::duplex(DUPLEX_CAPACITY_BYTES);
            let terminal = terminal();
            let framing = if chunked {
                "Transfer-Encoding: chunked".into()
            } else {
                format!("Content-Length: {}", HEARTBEAT_COUNT * b": alive\n".len() + terminal.len())
            };
            let server = async {
                peer.write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n{framing}\r\n\r\n"
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
                for _ in 0..HEARTBEAT_COUNT {
                    sleep(Duration::from_secs(HEARTBEAT_INTERVAL_SECONDS)).await;
                    peer.write_all(if chunked {
                        b"8;heartbeat\r\n: alive\n\r\n"
                    } else {
                        b": alive\n"
                    })
                    .await
                    .unwrap();
                }
                if chunked {
                    peer.write_all(format!("{:x}\r\n", terminal.len()).as_bytes()).await.unwrap();
                }
                peer.write_all(&terminal).await.unwrap();
                if chunked {
                    peer.write_all(b"\r\n0\r\n\r\n").await.unwrap();
                }
                peer.shutdown().await.unwrap();
            };
            let mut items = Vec::new();
            let mut limits = deadlines();
            limits.finite_total_milliseconds = SHORT_FINITE_TIMEOUT_MILLISECONDS;
            limits.finite_idle_milliseconds = SHORT_FINITE_TIMEOUT_MILLISECONDS;
            let (result, ()) = tokio::join!(
                receive(
                    &mut input,
                    "sub".into(),
                    TEST_GENERATION,
                    resolve,
                    |item| {
                        items.push(item);
                        Ok(())
                    },
                    limits
                ),
                server
            );
            assert!(matches!(result.unwrap(), EventHttpOutcome::Closed));
            assert_eq!(items.len(), 4);
            assert!(matches!(items.last().unwrap(), StreamItem::Event(_)));
        }
    }
    #[tokio::test]
    async fn malformed_tail_trailers_surplus_and_short_bodies_preserve_only_prior_items() {
        for defect in ["malformed", "partial", "trailer", "surplus", "short", "sink"] {
            let mut body = terminal();
            if defect == "malformed" {
                body.extend_from_slice(b"data: invalid\n\n");
            }
            if defect == "partial" {
                body.extend_from_slice(b"data: partial");
            }
            let mut wire = if defect == "trailer" {
                format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n", body.len()).into_bytes()
            } else {
                format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\n\r\n", body.len() + usize::from(defect == "short")).into_bytes()
            };
            wire.extend_from_slice(&body);
            if defect == "trailer" {
                wire.extend_from_slice(b"\r\n0\r\nx: forbidden\r\n\r\n");
            }
            if defect == "surplus" {
                wire.push(b'!');
            }
            let mut items = 0;
            let result = receive(
                &mut wire.as_slice(),
                "sub".into(),
                TEST_GENERATION,
                resolve,
                |_| {
                    if defect == "sink" {
                        return Err(FiniteHttpFailure::Body);
                    }
                    items += 1;
                    Ok(())
                },
                deadlines(),
            )
            .await;
            assert!(result.is_err(), "{defect}");
            assert_eq!(items, usize::from(defect != "sink"), "{defect}");
        }
    }
    #[tokio::test(start_paused = true)]
    async fn chunk_metadata_and_partial_event_bytes_do_not_refresh_heartbeat() {
        for metadata in [false, true] {
            let (mut input, mut peer) = tokio::io::duplex(DUPLEX_CAPACITY_BYTES);
            let server = async {
                peer.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\n").await.unwrap();
                if metadata {
                    peer.write_all(b"1;comment=\"").await.unwrap();
                }
                for _ in 0..ACTIVITY_WRITES {
                    sleep(Duration::from_secs(ACTIVITY_INTERVAL_SECONDS)).await;
                    if peer.write_all(if metadata { b"x" } else { b"1\r\nd\r\n" }).await.is_err() {
                        break;
                    }
                }
            };
            let client = async {
                let result = receive(
                    &mut input,
                    "sub".into(),
                    TEST_GENERATION,
                    resolve,
                    |_| panic!("partial item delivered"),
                    deadlines(),
                )
                .await;
                drop(input);
                result
            };
            let started = Instant::now();
            let (result, ()) = tokio::join!(client, server);
            assert_eq!(result.unwrap_err(), FiniteHttpFailure::EventHeartbeat);
            assert!(
                started.elapsed()
                    >= Duration::from_millis(
                        crate::event_stream_heartbeat::heartbeat_timeout_milliseconds()
                    )
            );
            assert!(
                started.elapsed()
                    <= Duration::from_secs(ACTIVITY_WRITES * ACTIVITY_INTERVAL_SECONDS)
            );
        }
    }
    #[tokio::test]
    async fn json_errors_remain_finite_and_forbidden_heads_deliver_nothing() {
        for (status, fields, body, valid) in [
            (409, "Content-Type: application/json\r\nContent-Length: 2", "{}", true),
            (
                410,
                "Content-Type: application/json\r\nTransfer-Encoding: chunked",
                "2\r\n{}\r\n0\r\n\r\n",
                true,
            ),
            (401, "Content-Type: application/json\r\nContent-Length: 2", "{}", true),
            (200, "Content-Type: application/json\r\nContent-Length: 2", "{}", false),
            (
                200,
                "Content-Type: text/event-stream\r\nContent-Encoding: gzip\r\nContent-Length: 0",
                "",
                false,
            ),
            (200, "Content-Type: text/event-stream\r\nTrailer: x\r\nContent-Length: 0", "", false),
            (
                200,
                "Content-Type: text/event-stream\r\nContent-Length: 0\r\nContent-Length: 0",
                "",
                false,
            ),
            (
                200,
                "Content-Type: text/event-stream\r\nContent-Length: 0\r\nTransfer-Encoding: chunked",
                "",
                false,
            ),
            (103, "Content-Length: 0", "", false),
            (302, "Content-Length: 0", "", false),
        ] {
            let bytes = format!("HTTP/1.1 {status} Response\r\n{fields}\r\n\r\n{body}");
            let result = receive(
                &mut bytes.as_bytes(),
                "sub".into(),
                TEST_GENERATION,
                resolve,
                |_| panic!("invalid head or error became event"),
                deadlines(),
            )
            .await;
            assert_eq!(result.is_ok(), valid);
            if valid {
                assert!(matches!(result.unwrap(), EventHttpOutcome::Response(_)));
            }
        }
    }
}
