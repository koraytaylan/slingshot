//! One selected-author event attachment. Events are delivered independently;
//! connection loss never retracts earlier committed items or settles a job.

use http::{Method, StatusCode, Version};
#[cfg(test)]
use slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract;
#[cfg(test)]
use tokio::time::Duration;
use tokio::time::Instant;

use crate::authentication::environment_provider::RequestAuthentication;
use crate::author_hypertext_transfer_protocol_policy::ExchangeDeadlines;
pub use crate::selected_author_events::EventHttpOutcome;
use crate::selected_author_events::{EventDelivery, classify_attachment, request_fields};
use crate::selected_author_exchange::validate_finite_head;
use crate::selected_author_hpack_block::ResponseBlock;
use crate::selected_author_http::FiniteHttpFailure;
use crate::selected_author_http2::{ResponseConsumer, drive_response};
use crate::selected_author_http2_flow::ReceiveWindows;
use crate::selected_author_http2_frames::{ResponseFrame, TransportEnd};
use crate::selected_author_http2_response::{FiniteResponse, ResponseRefusal, declared_length};
use crate::selected_author_transport::SelectedAuthorTransport;
use crate::server_sent_event_decoder::{
    EventStreamCursor, StreamItem, TerminalExpectationResolver,
};

#[cfg(test)]
use crate::server_sent_event_decoder::StreamRefusal;

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::selected_author_http2::tests::{close, deadlines, frame, handshake, receive};
    use crate::server_sent_event_decoder::OperationStreamExpectation;
    use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream};
    use tokio::time::{sleep, timeout};

    const OPERATION: &str = "1111111111111111111111111111111111111111111111111111111111111111";

    pub(crate) fn resolve(operation: &str) -> Result<OperationStreamExpectation, StreamRefusal> {
        if operation != OPERATION {
            return Err(StreamRefusal::AnotherSubmission);
        }
        Ok(OperationStreamExpectation {
            daemon_subscription_identifier: "sub".into(), agent_event_store_generation: 7,
            agent_operation_identifier: operation.into(),
            expected_provenance: slingshot_agent_protocol::wire_contract::ExpectedProvenance {
                command_contract: slingshot_domain::selected_command_contract_identity::SelectedCommandContractIdentity::installed("query_paths").unwrap(),
                canonical_json_contract_digest: slingshot_domain::command::schema::canonical_contract_digest(),
                transport_contract_digest: AuthorAgentTransportContract::embedded_digest(),
            },
            submitted_command_digest: "1".repeat(64),
        })
    }
    pub(crate) fn terminal() -> Vec<u8> {
        let retained = resolve(OPERATION).unwrap();
        let document = serde_json::json!({
            "agent_event_store_generation":7, "agent_operation_identifier":OPERATION,
            "daemon_subscription_identifier":"sub", "kind":"succeeded", "sequence":9,
            "sling_job_identifier":"job-one", "state":"succeeded",
            "terminal":{"provenance":retained.expected_provenance.provenance(), "submitted_command_digest":retained.submitted_command_digest}
        });
        format!("id: cursor-one\nevent: job-event\ndata: {document}\n\n").into_bytes()
    }
    fn head(status: &str, media: &str, extra: &[(&str, &str)]) -> Vec<u8> {
        let mut block = Vec::new();
        for (name, value) in
            [(":status", status), ("content-type", media)].into_iter().chain(extra.iter().copied())
        {
            block.extend_from_slice(&[0, name.len() as u8]);
            block.extend_from_slice(name.as_bytes());
            block.push(value.len() as u8);
            block.extend_from_slice(value.as_bytes());
        }
        frame(1, 4, 1, &block)
    }
    async fn client(
        mut stream: DuplexStream,
        consume: impl FnMut(StreamItem) -> Result<(), FiniteHttpFailure>,
    ) -> Result<EventHttpOutcome, FiniteHttpFailure> {
        let negotiated =
            crate::selected_author_http2_handshake::negotiate(&mut stream, Duration::from_secs(1))
                .await?;
        let mut limits = deadlines();
        limits.finite_total_milliseconds = 10;
        limits.finite_idle_milliseconds = 10;
        drive_response(
            stream,
            negotiated,
            [frame(1, 5, 1, &[0x82, 0x86, 0x84])].into_iter(),
            b"",
            limits,
            EventResponse::new("sub".into(), 7, resolve, consume),
        )
        .await
    }
    #[tokio::test(start_paused = true)]
    async fn a_live_stream_outlasts_finite_budgets_and_delivers_validated_terminal_items() {
        let (stream, mut peer) = tokio::io::duplex(4096);
        let mut delivered = Vec::new();
        let server = async {
            handshake(&mut peer, 65535).await;
            receive(&mut peer).await;
            peer.write_all(&head("200", "text/event-stream", &[])).await.unwrap();
            for _ in 0..3 {
                sleep(Duration::from_secs(20)).await;
                peer.write_all(&frame(0, 0, 1, b": alive\n")).await.unwrap();
                for _ in 0..2 {
                    assert_eq!(receive(&mut peer).await.kind, 8);
                }
            }
            peer.write_all(&frame(0, 1, 1, &terminal())).await.unwrap();
            close(&mut peer).await;
        };
        let (result, ()) = tokio::join!(
            client(stream, |item| {
                delivered.push(item);
                Ok(())
            }),
            server
        );
        assert!(matches!(result.unwrap(), EventHttpOutcome::Closed));
        assert_eq!(delivered.len(), 4);
        let StreamItem::Event(event) = delivered.last().unwrap() else {
            panic!("terminal missing");
        };
        assert_eq!(event.event.agent_operation_identifier, OPERATION);
        assert_eq!(event.cursor.as_ref().unwrap().as_text(), "cursor-one");
    }
    #[tokio::test]
    async fn later_malformed_tail_trailer_or_truncation_never_retracts_delivered_events() {
        for defect in ["malformed", "trailer", "partial", "wrong-operation", "sink"] {
            let (stream, mut peer) = tokio::io::duplex(4096);
            let mut delivered = 0;
            let server = async {
                handshake(&mut peer, 65535).await;
                receive(&mut peer).await;
                let mut body = terminal();
                if defect == "malformed" {
                    body.extend_from_slice(b"data: invalid\n\n");
                }
                if defect == "partial" {
                    body.extend_from_slice(b"data: incomplete");
                }
                if defect == "wrong-operation" {
                    body.extend_from_slice(
                        String::from_utf8(terminal())
                            .unwrap()
                            .replace(
                                &format!("\"agent_operation_identifier\":\"{OPERATION}\""),
                                &format!("\"agent_operation_identifier\":\"{}\"", "2".repeat(64)),
                            )
                            .as_bytes(),
                    );
                }
                let mut wire = head("200", "text/event-stream", &[]);
                wire.extend_from_slice(&frame(0, 1, 1, &body));
                if defect == "trailer" {
                    wire.extend_from_slice(&frame(1, 5, 1, &[]));
                }
                peer.write_all(&wire).await.unwrap();
                let mut outgoing = Vec::new();
                let _ = peer.read_to_end(&mut outgoing).await;
                peer.shutdown().await.unwrap();
            };
            let consumer = |_: StreamItem| {
                if defect == "sink" {
                    return Err(FiniteHttpFailure::Body);
                }
                delivered += 1;
                Ok(())
            };
            let (result, ()) = timeout(Duration::from_secs(2), async {
                tokio::join!(client(stream, consumer), server)
            })
            .await
            .unwrap();
            assert!(result.is_err(), "{defect}");
            assert_eq!(delivered, if defect == "sink" { 0 } else { 1 }, "{defect}");
        }
    }
    #[tokio::test(start_paused = true)]
    async fn ping_and_partial_event_traffic_cannot_refresh_application_heartbeat() {
        for ping in [true, false] {
            let (stream, mut peer) = tokio::io::duplex(4096);
            let server = async {
                handshake(&mut peer, 65535).await;
                receive(&mut peer).await;
                peer.write_all(&head("200", "text/event-stream", &[])).await.unwrap();
                loop {
                    sleep(Duration::from_secs(10)).await;
                    let bytes =
                        if ping { frame(6, 0, 0, b"12345678") } else { frame(0, 0, 1, b"d") };
                    if peer.write_all(&bytes).await.is_err() {
                        break;
                    }
                    let mut acknowledgement = vec![0; if ping { 17 } else { 26 }];
                    if peer.read_exact(&mut acknowledgement).await.is_err() {
                        break;
                    }
                }
            };
            let started = Instant::now();
            let (result, ()) =
                tokio::join!(client(stream, |_| panic!("incomplete item delivered")), server);
            assert_eq!(result.unwrap_err(), FiniteHttpFailure::EventHeartbeat);
            assert!(started.elapsed() >= Duration::from_secs(45));
            assert!(started.elapsed() <= Duration::from_secs(50));
        }
    }
    #[tokio::test]
    async fn non_stream_responses_remain_finite_documents_without_event_delivery() {
        for (status, media, valid) in [
            ("409", "application/json", true),
            ("410", "application/json", true),
            ("401", "application/json", true),
            ("200", "application/json", false),
            ("409", "text/event-stream", false),
        ] {
            let (stream, mut peer) = tokio::io::duplex(4096);
            let server = async {
                handshake(&mut peer, 65535).await;
                receive(&mut peer).await;
                let bytes = [head(status, media, &[]), frame(0, 1, 1, b"{}")].concat();
                peer.write_all(&bytes).await.unwrap();
                let mut outgoing = Vec::new();
                let _ = peer.read_to_end(&mut outgoing).await;
                peer.shutdown().await.unwrap();
            };
            let (result, ()) = timeout(Duration::from_secs(2), async {
                tokio::join!(client(stream, |_| panic!("error became event")), server)
            })
            .await
            .unwrap();
            assert_eq!(result.is_ok(), valid);
            if valid {
                let EventHttpOutcome::Response(response) = result.unwrap() else {
                    panic!("expected finite response");
                };
                assert_eq!(response.status.to_string(), status);
                assert_eq!(response.body, b"{}"); // not yet a validated reset document
            }
        }
    }

    #[tokio::test]
    async fn forbidden_heads_and_length_mismatches_fail_before_item_delivery() {
        for fields in [
            vec![("content-encoding", "gzip")],
            vec![("content-type", "text/event-stream")],
            vec![("trailer", "x")],
            vec![("location", "/alternate")],
            vec![("content-length", "2"), ("content-length", "2")],
            vec![("content-length", "+2")],
            vec![("content-length", "1")],
            vec![("content-length", "3")],
        ] {
            let (stream, mut peer) = tokio::io::duplex(4096);
            let server = async {
                handshake(&mut peer, 65535).await;
                receive(&mut peer).await;
                let bytes =
                    [head("200", "text/event-stream", &fields), frame(0, 1, 1, b":\n")].concat();
                peer.write_all(&bytes).await.unwrap();
                let mut outgoing = Vec::new();
                let _ = peer.read_to_end(&mut outgoing).await;
                peer.shutdown().await.unwrap();
            };
            let (result, ()) = timeout(Duration::from_secs(2), async {
                tokio::join!(
                    client(stream, |_| panic!("invalid response delivered an item")),
                    server
                )
            })
            .await
            .unwrap();
            assert!(result.is_err());
        }
    }
}

impl SelectedAuthorTransport {
    /// Attaches exactly once using the selected author and supplied committed
    /// subscription cursor. The caller owns durable per-item folding and must
    /// resolve terminal expectations from independently retained operations.
    /// No retry, cursor persistence, authentication refresh or job mutation is
    /// inferred by this transport. Dropping its future drops the attachment.
    pub async fn events_http2<R: TerminalExpectationResolver>(
        &self,
        identity: &slingshot_domain::operation_executor::ExecutionIdentity,
        subscription: &str,
        generation: u64,
        committed_cursor: Option<&EventStreamCursor>,
        authentication: &RequestAuthentication,
        resolver: R,
        consume: impl FnMut(StreamItem) -> Result<(), FiniteHttpFailure>,
    ) -> Result<EventHttpOutcome, FiniteHttpFailure> {
        self.events_over(
            identity,
            subscription,
            generation,
            committed_cursor,
            authentication,
            resolver,
            consume,
            false,
        )
        .await
    }

    /// Attaches on the original negotiated socket with the same committed
    /// cursor and retained terminal resolver. No retry or cursor advancement
    /// is inferred; dropping the future drops the selected attachment.
    pub async fn events_negotiated<R: TerminalExpectationResolver>(
        &self,
        identity: &slingshot_domain::operation_executor::ExecutionIdentity,
        subscription: &str,
        generation: u64,
        committed_cursor: Option<&EventStreamCursor>,
        authentication: &RequestAuthentication,
        resolver: R,
        consume: impl FnMut(StreamItem) -> Result<(), FiniteHttpFailure>,
    ) -> Result<EventHttpOutcome, FiniteHttpFailure> {
        self.events_over(
            identity,
            subscription,
            generation,
            committed_cursor,
            authentication,
            resolver,
            consume,
            true,
        )
        .await
    }

    /// Attaches with request-scoped provider authentication. A fully framed JSON
    /// 401 may refresh Cloud once and repeat the exact committed cursor. Once
    /// stream items have been delivered, no failure here retries the attachment.
    pub async fn events_authenticated<R: TerminalExpectationResolver>(
        &self,
        identity: &slingshot_domain::operation_executor::ExecutionIdentity,
        subscription: &str,
        generation: u64,
        committed_cursor: Option<&EventStreamCursor>,
        provider: &crate::authentication::environment_provider::EnvironmentAuthenticationProvider,
        source: &dyn crate::authentication::access_token_cache::AccessTokenSource,
        reading: u64,
        mut resolver: R,
        mut consume: impl FnMut(StreamItem) -> Result<(), FiniteHttpFailure>,
    ) -> Result<EventHttpOutcome, FiniteHttpFailure> {
        request_fields(self, identity, subscription, generation, committed_cursor)?;
        self.require_provider(provider).map_err(|_| FiniteHttpFailure::Request)?;
        let (authentication, lease) = provider
            .authenticate(&self.endpoint(&["bin", "slingshot-agent", "events"]), reading, source)
            .map_err(|_| FiniteHttpFailure::Request)?;
        let mut outcome = self
            .events_negotiated(
                identity,
                subscription,
                generation,
                committed_cursor,
                &authentication,
                |operation: &str| resolver.resolve(operation),
                &mut consume,
            )
            .await?;
        drop(authentication);
        if matches!(&outcome, EventHttpOutcome::Response(response) if response.status==401) {
            if let Some(lease) = lease {
                let (authentication, _) = provider
                    .refresh_after_unauthorized(lease, source)
                    .map_err(|_| FiniteHttpFailure::Head)?;
                outcome = self
                    .events_negotiated(
                        identity,
                        subscription,
                        generation,
                        committed_cursor,
                        &authentication,
                        |operation: &str| resolver.resolve(operation),
                        &mut consume,
                    )
                    .await?;
            }
        }
        Ok(outcome)
    }

    /// Uses the selected async provider; only a complete pre-stream 401
    /// permits one refreshed request. Partial consumer delivery never retries.
    pub async fn events_authenticated_async<R: TerminalExpectationResolver, Clock, Utc>(
        &self,
        identity: &slingshot_domain::operation_executor::ExecutionIdentity,
        subscription: &str,
        generation: u64,
        committed_cursor: Option<&EventStreamCursor>,
        provider: &crate::authentication::environment_provider::AsyncEnvironmentAuthenticationProvider,
        clock: &Clock,
        utc: &Utc,
        mut resolver: R,
        mut consume: impl FnMut(StreamItem) -> Result<(), FiniteHttpFailure>,
    ) -> Result<EventHttpOutcome, FiniteHttpFailure>
    where
        Clock: crate::authentication::identity_management_exchange::MonotonicClock + Sync,
        Utc: crate::authentication::token_assertion::CoordinatedUniversalTimeClock + Sync,
    {
        request_fields(self, identity, subscription, generation, committed_cursor)?;
        self.require_provider(provider).map_err(|_| FiniteHttpFailure::Request)?;
        let (authentication, lease) = provider
            .authenticate(&self.endpoint(&["bin", "slingshot-agent", "events"]), clock, utc)
            .await
            .map_err(|_| FiniteHttpFailure::Request)?;
        let mut outcome = self
            .events_negotiated(
                identity,
                subscription,
                generation,
                committed_cursor,
                &authentication,
                |operation: &str| resolver.resolve(operation),
                &mut consume,
            )
            .await?;
        drop(authentication);
        if matches!(&outcome, EventHttpOutcome::Response(response) if response.status==401) {
            if let Some(lease) = lease {
                let (authentication, _) = provider
                    .refresh_after_unauthorized(lease, clock, utc)
                    .await
                    .map_err(|_| FiniteHttpFailure::Head)?;
                outcome = self
                    .events_negotiated(
                        identity,
                        subscription,
                        generation,
                        committed_cursor,
                        &authentication,
                        |operation: &str| resolver.resolve(operation),
                        &mut consume,
                    )
                    .await?;
            }
        }
        Ok(outcome)
    }

    async fn events_over<R: TerminalExpectationResolver>(
        &self,
        identity: &slingshot_domain::operation_executor::ExecutionIdentity,
        subscription: &str,
        generation: u64,
        committed_cursor: Option<&EventStreamCursor>,
        authentication: &RequestAuthentication,
        resolver: R,
        consume: impl FnMut(StreamItem) -> Result<(), FiniteHttpFailure>,
        automatic: bool,
    ) -> Result<EventHttpOutcome, FiniteHttpFailure> {
        let fields = request_fields(self, identity, subscription, generation, committed_cursor)?;
        let generation_text = generation.to_string();
        let head = self.encode_http2_request_head(
            Method::GET,
            &["bin", "slingshot-agent", "events"],
            &[
                ("agent_event_store_generation", &generation_text),
                ("daemon_subscription_identifier", subscription),
            ],
            authentication,
            &fields,
            b"",
        );
        let http1 = if automatic {
            Some(crate::selected_author_http::encode_request(
                self,
                Method::GET,
                &["bin", "slingshot-agent", "events"],
                &[
                    ("agent_event_store_generation", &generation_text),
                    ("daemon_subscription_identifier", subscription),
                ],
                authentication,
                &fields,
                b"",
            ))
        } else {
            None
        };
        if head.is_err() && http1.as_ref().is_none_or(Result::is_err) {
            return Err(FiniteHttpFailure::Request);
        }
        let mut stream = if automatic {
            let (protocol, stream) = self
                .connect_negotiated()
                .await
                .map_err(|_| FiniteHttpFailure::Connect)?
                .into_parts();
            if protocol == crate::selected_author_transport::SelectedHttpProtocol::Http1 {
                return Self::events_http1_on_stream(
                    stream,
                    &http1.ok_or(FiniteHttpFailure::Request)??,
                    subscription,
                    generation,
                    committed_cursor,
                    resolver,
                    consume,
                )
                .await;
            }
            stream
        } else {
            self.connect_http2().await.map_err(|_| FiniteHttpFailure::Connect)?
        };
        let head = head?;
        let deadlines = ExchangeDeadlines::embedded();
        let negotiated = crate::selected_author_http2_handshake::negotiate(
            &mut stream,
            tokio::time::Duration::from_millis(deadlines.response_header_milliseconds),
        )
        .await?;
        drive_response(
            stream,
            negotiated,
            head.frames(),
            b"",
            deadlines,
            EventResponse::new(subscription.to_owned(), generation, resolver, consume),
        )
        .await
        .and_then(|outcome| {
            classify_attachment(outcome, subscription, generation, committed_cursor)
        })
    }
}

struct EventResponse<R, C> {
    subscription: String,
    generation: u64,
    resolver: Option<R>,
    consume: Option<C>,
    delivery: Option<EventDelivery<R, C>>,
    error: Option<FiniteResponse>,
    block: Option<ResponseBlock>,
    header_end: bool,
    complete_head: bool,
    ended: bool,
    length: Option<u64>,
    received: u64,
    windows: ReceiveWindows,

    poisoned: bool,
}

impl<R, C> EventResponse<R, C> {
    fn new(subscription: String, generation: u64, resolver: R, consume: C) -> Self {
        Self {
            subscription,
            generation,
            resolver: Some(resolver),
            consume: Some(consume),
            delivery: None,
            error: None,
            block: None,
            header_end: false,
            complete_head: false,
            ended: false,
            length: None,
            received: 0,
            windows: ReceiveWindows::new(),

            poisoned: false,
        }
    }
}

impl<R: TerminalExpectationResolver, C: FnMut(StreamItem) -> Result<(), FiniteHttpFailure>>
    ResponseConsumer for EventResponse<R, C>
{
    type Output = EventHttpOutcome;
    fn head_complete(&self) -> bool {
        self.complete_head && !self.poisoned
    }
    fn stream_ended(&self) -> bool {
        !self.poisoned && self.error.as_ref().map_or(self.ended, FiniteResponse::stream_ended)
    }
    fn liveness_deadline(&self) -> Option<Instant> {
        self.delivery.as_ref().map(EventDelivery::deadline)
    }
    fn accept(&mut self, frame: &ResponseFrame) -> Result<Option<[[u8; 13]; 2]>, ResponseRefusal> {
        if self.poisoned || self.stream_ended() || frame.stream_identifier != 1 {
            self.poisoned = true;
            return Err(ResponseRefusal);
        }
        self.poisoned = true;
        let credits = if let Some(error) = &mut self.error {
            error.accept(frame)?
        } else {
            match frame.kind {
                1 | 9 => {
                    if self.complete_head {
                        return Err(ResponseRefusal);
                    }
                    if frame.kind == 1 {
                        if self.block.is_some() {
                            return Err(ResponseRefusal);
                        }
                        self.block = Some(ResponseBlock::new());
                        self.header_end = frame.flags & 1 != 0;
                    }
                    self.block
                        .as_mut()
                        .ok_or(ResponseRefusal)?
                        .push(&frame.payload)
                        .map_err(|_| ResponseRefusal)?;
                    if frame.flags & 4 != 0 {
                        let (status, headers) = self
                            .block
                            .take()
                            .unwrap()
                            .finish()
                            .map_err(|_| ResponseRefusal)?
                            .into_parts();
                        let (head, media) = validate_finite_head(status, Version::HTTP_2, &headers)
                            .map_err(|_| ResponseRefusal)?;
                        if head.location.is_some() {
                            return Err(ResponseRefusal);
                        }
                        if status == StatusCode::OK {
                            self.length = declared_length(&headers)?;
                            if self.header_end && self.length.is_some_and(|length| length != 0) {
                                return Err(ResponseRefusal);
                            }
                            self.delivery = Some(
                                EventDelivery::attached(
                                    &head,
                                    &media,
                                    self.subscription.clone(),
                                    self.generation,
                                    self.resolver.take().ok_or(ResponseRefusal)?,
                                    self.consume.take().ok_or(ResponseRefusal)?,
                                )
                                .map_err(|_| ResponseRefusal)?,
                            );
                            self.ended = self.header_end;
                        } else {
                            if !crate::selected_author_submission::json_media_type(&media) {
                                return Err(ResponseRefusal);
                            }
                            self.error = Some(FiniteResponse::from_decoded_head(
                                status,
                                headers,
                                self.header_end,
                            )?);
                        }
                        self.complete_head = true;
                    }
                    None
                }
                0 => {
                    if !self.complete_head || self.block.is_some() {
                        return Err(ResponseRefusal);
                    }
                    let (permit, content) =
                        self.windows.receive_frame(frame).map_err(|_| ResponseRefusal)?;
                    let received =
                        self.received.checked_add(content.len() as u64).ok_or(ResponseRefusal)?;
                    if self.length.is_some_and(|length| {
                        received > length || frame.flags & 1 != 0 && received != length
                    }) {
                        return Err(ResponseRefusal);
                    }
                    self.delivery
                        .as_mut()
                        .ok_or(ResponseRefusal)?
                        .push(content)
                        .map_err(|_| ResponseRefusal)?;
                    self.received = received;
                    self.ended = frame.flags & 1 != 0;
                    permit.release()
                }
                _ => return Err(ResponseRefusal),
            }
        };
        self.poisoned = false;
        Ok(credits)
    }
    fn finish_at_transport_end(
        self,
        end: TransportEnd,
    ) -> Result<EventHttpOutcome, ResponseRefusal> {
        if !self.stream_ended() || self.block.is_some() {
            return Err(ResponseRefusal);
        }
        if let Some(error) = self.error {
            return error.finish_at_transport_end(end).map(EventHttpOutcome::Response);
        }
        self.delivery.ok_or(ResponseRefusal)?.finish().map_err(|_| ResponseRefusal)?;
        Ok(EventHttpOutcome::Closed)
    }
}
