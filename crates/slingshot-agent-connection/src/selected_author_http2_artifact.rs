//! Streaming artifacts over the same selected HTTP/2 driver and EOF proof.
//! The sink owns private staging, never publication; no successful receipt is
//! returned until framing, exact length and digest have all been verified.

use http::{HeaderMap, Method, StatusCode, Version};
use sha2::{Digest, Sha256};
use slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract;
use tokio::time::Instant;

use crate::artifact_download::{ArtifactResponseHead, ExpectedArtifact, require_streamable};
use crate::authentication::environment_provider::RequestAuthentication;
use crate::author_hypertext_transfer_protocol_policy::ExchangeDeadlines;
use crate::selected_author_exchange::{SelectedAuthorFiniteResponse, validate_finite_head};
use crate::selected_author_hpack_block::ResponseBlock;
use crate::selected_author_http::{ArtifactHttpOutcome, ArtifactHttpReceipt, FiniteHttpFailure};
use crate::selected_author_http2::{ResponseConsumer, drive_response};
use crate::selected_author_http2_flow::ReceiveWindows;
use crate::selected_author_http2_frames::{ResponseFrame, TransportEnd};
use crate::selected_author_http2_response::{FiniteResponse, ResponseRefusal, declared_length};
use crate::selected_author_transport::SelectedAuthorTransport;

impl SelectedAuthorTransport {
    /// Streams one manifest-bound artifact, a bounded 401, or a closed 404/410 refusal.
    /// Sink writes must remain private until the returned receipt is accepted.
    pub async fn artifact_http2(
        &self,
        identity: &slingshot_domain::operation_executor::ExecutionIdentity,
        submission: &crate::command_submission::Submission,
        expected: &ExpectedArtifact,
        artifact_identifier: &str,
        authentication: &RequestAuthentication,
        sink: impl FnMut(&[u8]) -> Result<(), FiniteHttpFailure>,
    ) -> Result<ArtifactHttpOutcome, FiniteHttpFailure> {
        self.artifact_over(
            identity,
            submission,
            expected,
            artifact_identifier,
            authentication,
            sink,
            false,
        )
        .await
    }

    /// Streams a manifest-bound artifact on the original negotiated socket.
    /// Both codecs retain private-sink, length/digest and closed-absence gates;
    /// no connection is retried or replaced after a refused transfer.
    pub async fn artifact_negotiated(
        &self,
        identity: &slingshot_domain::operation_executor::ExecutionIdentity,
        submission: &crate::command_submission::Submission,
        expected: &ExpectedArtifact,
        artifact_identifier: &str,
        authentication: &RequestAuthentication,
        sink: impl FnMut(&[u8]) -> Result<(), FiniteHttpFailure>,
    ) -> Result<ArtifactHttpOutcome, FiniteHttpFailure> {
        self.artifact_over(
            identity,
            submission,
            expected,
            artifact_identifier,
            authentication,
            sink,
            true,
        )
        .await
    }

    /// Streams with provider authentication, refreshing once only after a fully
    /// framed 401 that wrote no artifact bytes. Partial transfers never retry.
    pub async fn artifact_authenticated(
        &self,
        identity: &slingshot_domain::operation_executor::ExecutionIdentity,
        submission: &crate::command_submission::Submission,
        expected: &ExpectedArtifact,
        artifact_identifier: &str,
        provider: &crate::authentication::environment_provider::EnvironmentAuthenticationProvider,
        source: &dyn crate::authentication::access_token_cache::AccessTokenSource,
        reading: u64,
        mut sink: impl FnMut(&[u8]) -> Result<(), FiniteHttpFailure>,
    ) -> Result<ArtifactHttpOutcome, FiniteHttpFailure> {
        self.require_artifact_request(identity, submission, expected, artifact_identifier)?;
        self.require_provider(provider).map_err(|_| FiniteHttpFailure::Request)?;
        let started = Instant::now();
        let (authentication, lease) = provider
            .authenticate(
                &self.endpoint(&["bin", "slingshot-agent", "operations"]),
                reading,
                source,
            )
            .map_err(|_| FiniteHttpFailure::Request)?;
        let mut outcome = self
            .artifact_negotiated(
                identity,
                submission,
                expected,
                artifact_identifier,
                &authentication,
                &mut sink,
            )
            .await?;
        drop(authentication);
        if matches!(outcome, ArtifactHttpOutcome::Unauthorized) {
            if let Some(lease) = lease {
                let (authentication, _) = provider
                    .refresh_after_unauthorized(lease, source)
                    .map_err(|_| FiniteHttpFailure::Head)?;
                outcome = self
                    .artifact_negotiated(
                        identity,
                        submission,
                        expected,
                        artifact_identifier,
                        &authentication,
                        &mut sink,
                    )
                    .await?;
            }
        }
        let elapsed =
            u64::try_from(started.elapsed().as_nanos().div_ceil(1_000_000)).unwrap_or(u64::MAX);
        match outcome {
            ArtifactHttpOutcome::Transferred(receipt) => Ok(ArtifactHttpOutcome::Transferred(
                ArtifactHttpReceipt::verified(receipt.byte_length(), elapsed),
            )),
            ArtifactHttpOutcome::Unavailable { evidence, .. } => {
                Ok(ArtifactHttpOutcome::Unavailable { evidence, elapsed_milliseconds: elapsed })
            }
            ArtifactHttpOutcome::Unauthorized => Err(FiniteHttpFailure::Head),
        }
    }

    /// Uses the selected async provider; only a complete pre-stream 401
    /// permits one refreshed request. Partial consumer delivery never retries.
    pub async fn artifact_authenticated_async<Clock, Utc>(
        &self,
        identity: &slingshot_domain::operation_executor::ExecutionIdentity,
        submission: &crate::command_submission::Submission,
        expected: &ExpectedArtifact,
        artifact_identifier: &str,
        provider: &crate::authentication::environment_provider::AsyncEnvironmentAuthenticationProvider,
        clock: &Clock,
        utc: &Utc,
        mut sink: impl FnMut(&[u8]) -> Result<(), FiniteHttpFailure>,
    ) -> Result<ArtifactHttpOutcome, FiniteHttpFailure>
    where
        Clock: crate::authentication::identity_management_exchange::MonotonicClock + Sync,
        Utc: crate::authentication::token_assertion::CoordinatedUniversalTimeClock + Sync,
    {
        self.require_artifact_request(identity, submission, expected, artifact_identifier)?;
        self.require_provider(provider).map_err(|_| FiniteHttpFailure::Request)?;
        let started = Instant::now();
        let (authentication, lease) = provider
            .authenticate(&self.endpoint(&["bin", "slingshot-agent", "operations"]), clock, utc)
            .await
            .map_err(|_| FiniteHttpFailure::Request)?;
        let mut outcome = self
            .artifact_negotiated(
                identity,
                submission,
                expected,
                artifact_identifier,
                &authentication,
                &mut sink,
            )
            .await?;
        drop(authentication);
        if matches!(outcome, ArtifactHttpOutcome::Unauthorized) {
            if let Some(lease) = lease {
                let (authentication, _) = provider
                    .refresh_after_unauthorized(lease, clock, utc)
                    .await
                    .map_err(|_| FiniteHttpFailure::Head)?;
                outcome = self
                    .artifact_negotiated(
                        identity,
                        submission,
                        expected,
                        artifact_identifier,
                        &authentication,
                        &mut sink,
                    )
                    .await?;
            }
        }
        let elapsed =
            u64::try_from(started.elapsed().as_nanos().div_ceil(1_000_000)).unwrap_or(u64::MAX);
        match outcome {
            ArtifactHttpOutcome::Transferred(receipt) => Ok(ArtifactHttpOutcome::Transferred(
                ArtifactHttpReceipt::verified(receipt.byte_length(), elapsed),
            )),
            ArtifactHttpOutcome::Unavailable { evidence, .. } => {
                Ok(ArtifactHttpOutcome::Unavailable { evidence, elapsed_milliseconds: elapsed })
            }
            ArtifactHttpOutcome::Unauthorized => Err(FiniteHttpFailure::Head),
        }
    }

    fn require_artifact_request(
        &self,
        identity: &slingshot_domain::operation_executor::ExecutionIdentity,
        submission: &crate::command_submission::Submission,
        expected: &ExpectedArtifact,
        artifact_identifier: &str,
    ) -> Result<(), FiniteHttpFailure> {
        self.require_submission(identity, submission).map_err(|_| FiniteHttpFailure::Request)?;
        let media = crate::artifact_download::require_remote_slot(&expected.artifact_slot)
            .map_err(|_| FiniteHttpFailure::Request)?;
        if media != expected.media_type
            || expected.artifact_digest.len() != 64
            || !expected
                .artifact_digest
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || expected.byte_length
                > slingshot_domain::daemon_runtime_contract::DaemonRuntimeContract::embedded()
                    .formula("maximum_individual_artifact_bytes")
            || artifact_identifier.is_empty()
            || artifact_identifier.len() > 128
        {
            return Err(FiniteHttpFailure::Request);
        }
        Ok(())
    }

    async fn artifact_over(
        &self,
        identity: &slingshot_domain::operation_executor::ExecutionIdentity,
        submission: &crate::command_submission::Submission,
        expected: &ExpectedArtifact,
        artifact_identifier: &str,
        authentication: &RequestAuthentication,
        sink: impl FnMut(&[u8]) -> Result<(), FiniteHttpFailure>,
        automatic: bool,
    ) -> Result<ArtifactHttpOutcome, FiniteHttpFailure> {
        self.require_artifact_request(identity, submission, expected, artifact_identifier)?;
        let segments = [
            "bin",
            "slingshot-agent",
            "operations",
            &submission.operation.agent_operation_identifier,
            "artifacts",
            &expected.artifact_slot,
        ];
        let head = self.encode_http2_request_head(
            Method::GET,
            &segments,
            &[],
            authentication,
            &HeaderMap::new(),
            b"",
        );
        let http1 = if automatic {
            Some(crate::selected_author_http::encode_request(
                self,
                Method::GET,
                &segments,
                &[],
                authentication,
                &HeaderMap::new(),
                b"",
            ))
        } else {
            None
        };
        if head.is_err() && http1.as_ref().is_none_or(Result::is_err) {
            return Err(FiniteHttpFailure::Request);
        }
        let started = Instant::now();
        let mut stream = if automatic {
            let (protocol, stream) = self
                .connect_negotiated()
                .await
                .map_err(|_| FiniteHttpFailure::Connect)?
                .into_parts();
            if protocol == crate::selected_author_transport::SelectedHttpProtocol::Http1 {
                return Self::artifact_http1_on_stream(
                    stream,
                    &http1.ok_or(FiniteHttpFailure::Request)??,
                    started,
                    submission,
                    expected,
                    Some(artifact_identifier),
                    sink,
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
        let output = drive_response(
            stream,
            negotiated,
            head.frames(),
            b"",
            deadlines,
            ArtifactResponse::new(expected, sink),
        )
        .await?;
        let elapsed_milliseconds =
            u64::try_from(started.elapsed().as_nanos().div_ceil(1_000_000)).unwrap_or(u64::MAX);
        match output {
            ArtifactBody::Transferred(byte_length) => Ok(ArtifactHttpOutcome::Transferred(
                ArtifactHttpReceipt::verified(byte_length, elapsed_milliseconds),
            )),
            ArtifactBody::Unavailable(response) => {
                if response.status == 401 {
                    return Ok(ArtifactHttpOutcome::Unauthorized);
                }
                let evidence = crate::artifact_download::decode_artifact_unavailable(
                    response.status,
                    &response.body,
                    submission,
                    artifact_identifier,
                    &expected.artifact_slot,
                )
                .map_err(|_| FiniteHttpFailure::Body)?;
                Ok(ArtifactHttpOutcome::Unavailable { evidence, elapsed_milliseconds })
            }
        }
    }
}

#[derive(Debug)]
enum ArtifactBody {
    Transferred(u64),
    Unavailable(SelectedAuthorFiniteResponse),
}

struct ArtifactResponse<'a, S> {
    expected: &'a ExpectedArtifact,
    sink: S,
    block: Option<ResponseBlock>,
    header_end: bool,
    head_complete: bool,
    ended: bool,
    unavailable: Option<FiniteResponse>,
    windows: ReceiveWindows,
    received: u64,
    digest: Sha256,
    poisoned: bool,
}

impl<'a, S> ArtifactResponse<'a, S> {
    fn new(expected: &'a ExpectedArtifact, sink: S) -> Self {
        Self {
            expected,
            sink,
            block: None,
            header_end: false,
            head_complete: false,
            ended: false,
            unavailable: None,
            windows: ReceiveWindows::new(),
            received: 0,
            digest: Sha256::new(),
            poisoned: false,
        }
    }
}

impl<S: FnMut(&[u8]) -> Result<(), FiniteHttpFailure>> ResponseConsumer
    for ArtifactResponse<'_, S>
{
    type Output = ArtifactBody;
    fn head_complete(&self) -> bool {
        self.head_complete && !self.poisoned
    }
    fn stream_ended(&self) -> bool {
        !self.poisoned && self.unavailable.as_ref().map_or(self.ended, FiniteResponse::stream_ended)
    }
    fn body_deadlines(&self, defaults: ExchangeDeadlines) -> (u64, u64) {
        if self.unavailable.is_some() {
            return (defaults.finite_total_milliseconds, defaults.finite_idle_milliseconds);
        }
        let contract = AuthorAgentTransportContract::embedded();
        (
            contract.limit("artifact_transfer_total_timeout_milliseconds"),
            contract.limit("artifact_transfer_idle_timeout_milliseconds"),
        )
    }
    fn accept(&mut self, frame: &ResponseFrame) -> Result<Option<[[u8; 13]; 2]>, ResponseRefusal> {
        if self.poisoned || self.stream_ended() || frame.stream_identifier != 1 {
            self.poisoned = true;
            return Err(ResponseRefusal);
        }
        self.poisoned = true;
        let credits = if let Some(unavailable) = &mut self.unavailable {
            unavailable.accept(frame)?
        } else {
            match frame.kind {
                1 | 9 => {
                    if self.head_complete {
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
                        let (head, content_type) =
                            validate_finite_head(status, Version::HTTP_2, &headers)
                                .map_err(|_| ResponseRefusal)?;
                        if status == StatusCode::OK {
                            require_streamable(
                                self.expected,
                                &ArtifactResponseHead { head, content_type },
                            )
                            .map_err(|_| ResponseRefusal)?;
                            if declared_length(&headers)?
                                .is_some_and(|length| length != self.expected.byte_length)
                                || self.header_end && self.expected.byte_length != 0
                            {
                                return Err(ResponseRefusal);
                            }
                            self.ended = self.header_end;
                        } else {
                            if !matches!(status.as_u16(), 401 | 404 | 410)
                                || head.location.is_some()
                                || !crate::selected_author_submission::json_media_type(
                                    &content_type,
                                )
                            {
                                return Err(ResponseRefusal);
                            }
                            self.unavailable = Some(FiniteResponse::from_decoded_head(
                                status,
                                headers,
                                self.header_end,
                            )?);
                        }
                        self.head_complete = true;
                    }
                    None
                }
                0 => {
                    if !self.head_complete || self.block.is_some() {
                        return Err(ResponseRefusal);
                    }
                    let (permit, content) =
                        self.windows.receive_frame(frame).map_err(|_| ResponseRefusal)?;
                    let length = self
                        .received
                        .checked_add(content.len() as u64)
                        .filter(|length| *length <= self.expected.byte_length)
                        .ok_or(ResponseRefusal)?;
                    if frame.flags & 1 != 0 && length != self.expected.byte_length {
                        return Err(ResponseRefusal);
                    }
                    if !content.is_empty() {
                        (self.sink)(content).map_err(|_| ResponseRefusal)?;
                    }
                    self.digest.update(content);
                    self.received = length;
                    self.ended = frame.flags & 1 != 0;
                    permit.release()
                }
                _ => return Err(ResponseRefusal),
            }
        };
        self.poisoned = false;
        Ok(credits)
    }
    fn finish_at_transport_end(self, end: TransportEnd) -> Result<ArtifactBody, ResponseRefusal> {
        if !self.stream_ended() || self.block.is_some() {
            return Err(ResponseRefusal);
        }
        if let Some(unavailable) = self.unavailable {
            return unavailable.finish_at_transport_end(end).map(ArtifactBody::Unavailable);
        }
        let digest: String =
            self.digest.finalize().iter().map(|byte| format!("{byte:02x}")).collect();
        if self.received != self.expected.byte_length || digest != self.expected.artifact_digest {
            return Err(ResponseRefusal);
        }
        Ok(ArtifactBody::Transferred(self.received))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expected(bytes: &[u8]) -> ExpectedArtifact {
        ExpectedArtifact {
            artifact_digest: Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect(),
            artifact_slot: "content_package".into(),
            byte_length: bytes.len() as u64,
            media_type: "application/zip".into(),
        }
    }
    fn head(status: &str, fields: &[(&str, &str)], ended: bool) -> ResponseFrame {
        let mut payload = Vec::new();
        for (name, value) in core::iter::once((":status", status)).chain(fields.iter().copied()) {
            payload.extend_from_slice(&[0, name.len() as u8]);
            payload.extend_from_slice(name.as_bytes());
            payload.push(value.len() as u8);
            payload.extend_from_slice(value.as_bytes());
        }
        ResponseFrame { kind: 1, flags: if ended { 5 } else { 4 }, stream_identifier: 1, payload }
    }
    fn data(bytes: &[u8], flags: u8) -> ResponseFrame {
        ResponseFrame { kind: 0, flags, stream_identifier: 1, payload: bytes.to_vec() }
    }
    #[test]
    fn streaming_counts_padding_and_only_returns_verified_length_after_eof() {
        let expected = expected(b"abc");
        let mut staged = Vec::new();
        let mut response = ArtifactResponse::new(&expected, |bytes: &[u8]| {
            staged.extend_from_slice(bytes);
            Ok(())
        });
        response
            .accept(&head(
                "200",
                &[("content-type", "application/zip"), ("content-length", "3")],
                false,
            ))
            .unwrap();
        let credit = response.accept(&data(&[1, b'a', 0], 8)).unwrap().unwrap();
        assert_eq!(&credit[0][9..], &[0, 0, 0, 3]);
        response.accept(&data(b"bc", 1)).unwrap();
        assert!(matches!(
            response.finish_at_transport_end(TransportEnd::for_test()).unwrap(),
            ArtifactBody::Transferred(3)
        ));
        assert_eq!(staged, b"abc");
    }
    #[test]
    fn invalid_head_never_touches_private_staging() {
        let expected = expected(b"abc");
        for fields in [
            vec![("content-type", "text/plain")],
            vec![("content-type", "application/zip"), ("content-length", "4")],
            vec![
                ("content-type", "application/zip"),
                ("content-length", "3"),
                ("content-length", "3"),
            ],
            vec![("content-type", "application/zip"), ("content-encoding", "gzip")],
            vec![("content-type", "application/zip"), ("trailer", "x")],
            vec![("content-type", "application/zip"), ("location", "/alternate")],
        ] {
            let mut response =
                ArtifactResponse::new(&expected, |_: &[u8]| -> Result<(), FiniteHttpFailure> {
                    panic!("invalid head streamed");
                });
            assert!(response.accept(&head("200", &fields, false)).is_err());
            assert!(response.finish_at_transport_end(TransportEnd::for_test()).is_err());
        }
    }
    #[test]
    fn short_excess_digest_drift_sink_refusal_and_post_end_input_never_verify() {
        let expected = expected(b"abc");
        for (bytes, refusal, trailing) in [
            (b"ab".as_slice(), false, false),
            (b"abcd", false, false),
            (b"abd", false, false),
            (b"abc", true, false),
            (b"abc", false, true),
        ] {
            let mut response = ArtifactResponse::new(&expected, |_: &[u8]| {
                if refusal { Err(FiniteHttpFailure::Body) } else { Ok(()) }
            });
            response.accept(&head("200", &[("content-type", "application/zip")], false)).unwrap();
            let _ = response.accept(&data(bytes, 1));
            if trailing {
                assert!(response.accept(&data(b"", 1)).is_err());
            }
            assert!(response.finish_at_transport_end(TransportEnd::for_test()).is_err());
        }
    }
    #[test]
    fn unavailable_responses_use_finite_limits_without_artifact_sink_writes() {
        let expected = expected(b"abc");
        for status in ["401", "404", "410"] {
            let mut response =
                ArtifactResponse::new(&expected, |_: &[u8]| -> Result<(), FiniteHttpFailure> {
                    panic!("error body entered staging");
                });
            response
                .accept(&head(
                    status,
                    &[("content-type", "application/json"), ("content-length", "2")],
                    false,
                ))
                .unwrap();
            response.accept(&data(b"{}", 1)).unwrap();
            let defaults = ExchangeDeadlines::embedded();
            assert_eq!(
                response.body_deadlines(defaults),
                (defaults.finite_total_milliseconds, defaults.finite_idle_milliseconds)
            );
            let ArtifactBody::Unavailable(result) =
                response.finish_at_transport_end(TransportEnd::for_test()).unwrap()
            else {
                panic!("wrong output");
            };
            assert_eq!(result.body, b"{}"); // wrapper must still perform closed identity decoding
        }
    }

    #[test]
    fn large_artifacts_exceed_finite_document_bounds_without_body_collection() {
        let maximum =
            AuthorAgentTransportContract::embedded().limit("maximum_finite_response_body_bytes");
        let size = maximum + 1;
        let chunk = vec![b'x'; 16_384];
        let mut digest = Sha256::new();
        let mut remaining = size;
        while remaining != 0 {
            let count = remaining.min(chunk.len() as u64) as usize;
            digest.update(&chunk[..count]);
            remaining -= count as u64;
        }
        let expected = ExpectedArtifact {
            artifact_digest: digest.finalize().iter().map(|b| format!("{b:02x}")).collect(),
            artifact_slot: "content_package".into(),
            byte_length: size,
            media_type: "application/zip".into(),
        };
        let mut staged = 0_u64;
        let mut response = ArtifactResponse::new(&expected, |bytes: &[u8]| {
            assert!(bytes.len() <= 16_384);
            staged += bytes.len() as u64;
            Ok(())
        });
        response
            .accept(&head(
                "200",
                &[("content-type", "application/zip"), ("content-length", &size.to_string())],
                false,
            ))
            .unwrap();
        let mut remaining = size;
        while remaining != 0 {
            let count = remaining.min(chunk.len() as u64) as usize;
            remaining -= count as u64;
            response.accept(&data(&chunk[..count], u8::from(remaining == 0))).unwrap();
        }
        assert!(
            matches!(response.finish_at_transport_end(TransportEnd::for_test()).unwrap(), ArtifactBody::Transferred(length) if length == size)
        );
        assert_eq!(staged, size);
    }

    #[test]
    fn fragmented_empty_artifact_head_still_requires_complete_header_block() {
        let expected = expected(b"");
        let mut response =
            ArtifactResponse::new(&expected, |_: &[u8]| -> Result<(), FiniteHttpFailure> {
                panic!("empty artifact streamed");
            });
        let mut initial = head("200", &[("content-type", "application/zip")], true);
        let tail = initial.payload.split_off(4);
        initial.flags = 1;
        response.accept(&initial).unwrap();
        assert!(!response.head_complete());
        assert!(!response.stream_ended());
        response
            .accept(&ResponseFrame { kind: 9, flags: 4, stream_identifier: 1, payload: tail })
            .unwrap();
        assert!(matches!(
            response.finish_at_transport_end(TransportEnd::for_test()).unwrap(),
            ArtifactBody::Transferred(0)
        ));
    }
}
