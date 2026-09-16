//! HTTP/1 artifact transfers with bounded refusal bodies and verified staging.

use super::{
    ArtifactHttpOutcome, ArtifactHttpReceipt, BodyFraming, FiniteHttpFailure, decode_chunk_size,
    encode_request, read_chunk_line, read_framed_body, read_head,
};
use crate::authentication::environment_provider::RequestAuthentication;
use crate::author_hypertext_transfer_protocol_policy::ExchangeDeadlines;
use crate::selected_author_exchange::{
    CollectedFiniteResponse, validate_collected_finite_response,
};
use crate::selected_author_transport::{SelectedAuthorStream, SelectedAuthorTransport};
use http::{HeaderMap, Method, Response, StatusCode, Version};
use slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::{Duration, Instant, timeout};

const ARTIFACT_IDENTIFIER_BYTES: usize = 128;
const NANOSECONDS_PER_MILLISECOND: u128 = 1_000_000;
const STREAM_BUFFER_BYTES: usize = 8192;

impl SelectedAuthorTransport {
    /// Streams a successful artifact response into private caller-owned staging.
    /// The caller validates the command/slot manifest and reserves capacity
    /// before calling. Sink writes must be bounded and must not publish content.
    /// Non-200 responses remain refusals, never proof of artifact retirement.
    ///
    /// # Errors
    /// Refuses invalid selection or artifact declarations, connection/write
    /// failures, non-success responses, invalid framing, expired deadlines,
    /// sink refusals, and length or digest mismatches. No partial receipt is returned.
    pub async fn stream_artifact_http1(
        &self,
        identity: &slingshot_domain::operation_executor::ExecutionIdentity,
        submission: &crate::command_submission::Submission,
        expected: &crate::artifact_download::ExpectedArtifact,
        authentication: &RequestAuthentication,
        sink: impl FnMut(&[u8]) -> Result<(), FiniteHttpFailure>,
    ) -> Result<ArtifactHttpReceipt, FiniteHttpFailure> {
        match self
            .exchange_artifact_http1(identity, submission, expected, None, authentication, sink)
            .await?
        {
            ArtifactHttpOutcome::Transferred(receipt) => Ok(receipt),
            ArtifactHttpOutcome::Unavailable { .. } | ArtifactHttpOutcome::Unauthorized => {
                Err(FiniteHttpFailure::Head)
            }
        }
    }

    /// Streams successful bytes or validates a closed unavailable response for
    /// the independently derived artifact identifier. Neither branch publishes.
    ///
    /// # Errors
    /// Refuses invalid identifiers or declarations, connection/write failures,
    /// invalid response framing or retirement evidence, expired deadlines, sink
    /// refusals, and length or digest mismatches.
    pub async fn artifact_http1(
        &self,
        identity: &slingshot_domain::operation_executor::ExecutionIdentity,
        submission: &crate::command_submission::Submission,
        expected: &crate::artifact_download::ExpectedArtifact,
        artifact_identifier: &str,
        authentication: &RequestAuthentication,
        sink: impl FnMut(&[u8]) -> Result<(), FiniteHttpFailure>,
    ) -> Result<ArtifactHttpOutcome, FiniteHttpFailure> {
        if artifact_identifier.is_empty() || artifact_identifier.len() > ARTIFACT_IDENTIFIER_BYTES {
            return Err(FiniteHttpFailure::Request);
        }
        self.exchange_artifact_http1(
            identity,
            submission,
            expected,
            Some(artifact_identifier),
            authentication,
            sink,
        )
        .await
    }

    async fn exchange_artifact_http1(
        &self,
        identity: &slingshot_domain::operation_executor::ExecutionIdentity,
        submission: &crate::command_submission::Submission,
        expected: &crate::artifact_download::ExpectedArtifact,
        artifact_identifier: Option<&str>,
        authentication: &RequestAuthentication,
        sink: impl FnMut(&[u8]) -> Result<(), FiniteHttpFailure>,
    ) -> Result<ArtifactHttpOutcome, FiniteHttpFailure> {
        self.require_submission(identity, submission).map_err(|_| FiniteHttpFailure::Request)?;
        let media = crate::artifact_download::require_remote_slot(&expected.artifact_slot)
            .map_err(|_| FiniteHttpFailure::Request)?;
        if media != expected.media_type
            || expected.artifact_digest.len()
                != slingshot_domain::command_fingerprint::DIGEST_CHARACTERS
            || !expected
                .artifact_digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            || expected.byte_length
                > slingshot_domain::daemon_runtime_contract::DaemonRuntimeContract::embedded()
                    .formula("maximum_individual_artifact_bytes")
        {
            return Err(FiniteHttpFailure::Request);
        }
        let request = encode_request(
            self,
            Method::GET,
            &["bin", "slingshot", "agent", "artifact"],
            &[
                ("agent_operation_identifier", &submission.operation.agent_operation_identifier),
                ("artifact_slot", &expected.artifact_slot),
            ],
            authentication,
            &HeaderMap::new(),
            b"",
        )?;
        let started = Instant::now();
        let stream = self.connect().await.map_err(|_| FiniteHttpFailure::Connect)?;
        Self::artifact_http1_on_stream(
            stream,
            &request,
            started,
            submission,
            expected,
            artifact_identifier,
            sink,
        )
        .await
    }

    pub(crate) async fn artifact_http1_on_stream(
        mut stream: SelectedAuthorStream,
        request: &[u8],
        started: Instant,
        submission: &crate::command_submission::Submission,
        expected: &crate::artifact_download::ExpectedArtifact,
        artifact_identifier: Option<&str>,
        mut sink: impl FnMut(&[u8]) -> Result<(), FiniteHttpFailure>,
    ) -> Result<ArtifactHttpOutcome, FiniteHttpFailure> {
        use sha2::Digest as _;
        let deadlines = ExchangeDeadlines::embedded();
        timeout(Duration::from_millis(deadlines.request_body_milliseconds), async {
            stream.write_all(request).await?;
            stream.flush().await
        })
        .await
        .map_err(|_| FiniteHttpFailure::Write)?
        .map_err(|_| FiniteHttpFailure::Write)?;
        let (status, headers, framing) = timeout(
            Duration::from_millis(deadlines.response_header_milliseconds),
            read_head(&mut stream),
        )
        .await
        .map_err(|_| FiniteHttpFailure::Head)??;
        let mut response = Response::builder()
            .status(status)
            .version(Version::HTTP_11)
            .body(Vec::new())
            .map_err(|_| FiniteHttpFailure::Head)?;
        *response.headers_mut() = headers;
        let accepted = validate_collected_finite_response(CollectedFiniteResponse {
            response,
            framing_ambiguous: false,
            trailer_section_present: false,
            trailing_bytes: false,
        })
        .map_err(|_| FiniteHttpFailure::Head)?;
        if status != StatusCode::OK.as_u16() {
            return refused_artifact_response(
                &mut stream,
                accepted,
                framing,
                submission,
                expected,
                artifact_identifier,
                started,
            )
            .await;
        }
        crate::artifact_download::require_streamable(
            expected,
            &crate::artifact_download::ArtifactResponseHead {
                head: accepted.head,
                content_type: accepted.content_type.ok_or(FiniteHttpFailure::Head)?,
            },
        )
        .map_err(|_| FiniteHttpFailure::Head)?;
        if matches!(framing, BodyFraming::Fixed(length) if length != expected.byte_length) {
            return Err(FiniteHttpFailure::Head);
        }
        let contract = AuthorAgentTransportContract::embedded();
        let idle =
            Duration::from_millis(contract.limit("artifact_transfer_idle_timeout_milliseconds"));
        let total =
            Duration::from_millis(contract.limit("artifact_transfer_total_timeout_milliseconds"));
        let mut received = 0_u64;
        let mut hasher = sha2::Sha256::new();
        let body_started = Instant::now();
        timeout(total, async {
            match framing {
                BodyFraming::Fixed(length) => {
                    stream_artifact_part(
                        &mut stream,
                        length,
                        expected.byte_length,
                        &mut received,
                        &mut hasher,
                        &mut sink,
                        idle,
                    )
                    .await?
                }
                BodyFraming::Chunked => loop {
                    let line = read_chunk_line(&mut stream, idle).await?;
                    let length = decode_chunk_size(&line)?;
                    if length == 0 {
                        if !read_chunk_line(&mut stream, idle).await?.is_empty() {
                            return Err(FiniteHttpFailure::Body);
                        }
                        break;
                    }
                    stream_artifact_part(
                        &mut stream,
                        length,
                        expected.byte_length,
                        &mut received,
                        &mut hasher,
                        &mut sink,
                        idle,
                    )
                    .await?;
                    if !read_chunk_line(&mut stream, idle).await?.is_empty() {
                        return Err(FiniteHttpFailure::Body);
                    }
                },
            }
            let mut extra = [0_u8];
            if timeout(idle, stream.read(&mut extra))
                .await
                .map_err(|_| FiniteHttpFailure::Body)?
                .map_err(|_| FiniteHttpFailure::Body)?
                != 0
            {
                return Err(FiniteHttpFailure::Body);
            }
            Ok::<(), FiniteHttpFailure>(())
        })
        .await
        .map_err(|_| FiniteHttpFailure::Body)??;
        let digest: String = hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect();
        require_complete_artifact(received, &digest, expected, body_started, total)?;
        Ok(ArtifactHttpOutcome::Transferred(ArtifactHttpReceipt {
            byte_length: received,
            elapsed_milliseconds: u64::try_from(
                started.elapsed().as_nanos().div_ceil(NANOSECONDS_PER_MILLISECOND),
            )
            .unwrap_or(u64::MAX),
        }))
    }
}

fn require_complete_artifact(
    received: u64,
    digest: &str,
    expected: &crate::artifact_download::ExpectedArtifact,
    body_started: Instant,
    total: Duration,
) -> Result<(), FiniteHttpFailure> {
    // A synchronous staging sink cannot be preempted by Tokio's timeout.
    // Refuse an overdue transfer even if its final poll completed ready.
    if body_started.elapsed() >= total
        || received != expected.byte_length
        || digest != expected.artifact_digest
    {
        return Err(FiniteHttpFailure::Body);
    }
    Ok(())
}

async fn refused_artifact_response(
    stream: &mut SelectedAuthorStream,
    accepted: crate::selected_author_exchange::SelectedAuthorFiniteResponse,
    framing: BodyFraming,
    submission: &crate::command_submission::Submission,
    expected: &crate::artifact_download::ExpectedArtifact,
    artifact_identifier: Option<&str>,
    started: Instant,
) -> Result<ArtifactHttpOutcome, FiniteHttpFailure> {
    let status = accepted.status;
    let deadlines = ExchangeDeadlines::embedded();
    let artifact_identifier = artifact_identifier.ok_or(FiniteHttpFailure::Head)?;
    if !matches!(status, 401 | 404 | 410)
        || accepted.head.location.is_some()
        || !crate::selected_author_submission::json_media_type(
            accepted.content_type.as_deref().unwrap_or(""),
        )
    {
        return Err(FiniteHttpFailure::Head);
    }
    let contract = AuthorAgentTransportContract::embedded();
    let limit = contract
        .limit("maximum_finite_response_body_bytes")
        .min(contract.limit("maximum_agent_protocol_document_bytes"));
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
    if status == StatusCode::UNAUTHORIZED.as_u16() {
        return Ok(ArtifactHttpOutcome::Unauthorized);
    }
    let evidence = crate::artifact_download::decode_artifact_unavailable(
        status,
        &body,
        submission,
        artifact_identifier,
        &expected.artifact_slot,
    )
    .map_err(|_| FiniteHttpFailure::Body)?;
    Ok(ArtifactHttpOutcome::Unavailable {
        evidence,
        elapsed_milliseconds: u64::try_from(
            started.elapsed().as_nanos().div_ceil(NANOSECONDS_PER_MILLISECOND),
        )
        .unwrap_or(u64::MAX),
    })
}

async fn stream_artifact_part(
    stream: &mut SelectedAuthorStream,
    length: u64,
    allowed: u64,
    received: &mut u64,
    hasher: &mut sha2::Sha256,
    sink: &mut impl FnMut(&[u8]) -> Result<(), FiniteHttpFailure>,
    idle: Duration,
) -> Result<(), FiniteHttpFailure> {
    use sha2::Digest as _;
    let end = received
        .checked_add(length)
        .filter(|end| *end <= allowed)
        .ok_or(FiniteHttpFailure::Body)?;
    let mut buffer = [0_u8; STREAM_BUFFER_BYTES];
    while *received < end {
        let wanted = usize::try_from(end - *received).unwrap_or(buffer.len()).min(buffer.len());
        let count = timeout(idle, stream.read(&mut buffer[..wanted]))
            .await
            .map_err(|_| FiniteHttpFailure::Body)?
            .map_err(|_| FiniteHttpFailure::Body)?;
        if count == 0 {
            return Err(FiniteHttpFailure::Body);
        }
        sink(&buffer[..count])?;
        hasher.update(&buffer[..count]);
        *received += count as u64;
    }
    Ok(())
}
