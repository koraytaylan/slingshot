//! Finite HTTP/1.1 exchanges over the frozen selected-author connector.
//!
//! A connection is used once and asks the peer to close. This lets the reader
//! prove that no bytes followed the framed message before publishing evidence.
//! This reader accepts explicit Content-Length and chunked framing;
//! HTTP/2 exchanges require their own reader before runtime composition
//! can advertise full transport conformance.

use http::{HeaderMap, HeaderName, HeaderValue, Method, Response, Uri, Version};
use slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::time::{Duration, Instant, timeout};

use crate::authentication::environment_provider::RequestAuthentication;
use crate::author_hypertext_transfer_protocol_policy::{ExchangeDeadlines, HeadBounds, HeadReader};
use crate::selected_author_exchange::{
    CollectedFiniteResponse, SelectedAuthorFiniteResponse, validate_collected_finite_response,
};
use crate::selected_author_transport::{SelectedAuthorStream, SelectedAuthorTransport};

/// Phase at which a finite exchange failed. No remote strings are retained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum FiniteHttpFailure {
    /// Request construction failed before network access.
    #[error("author request is invalid")]
    Request,
    /// No application request was sent.
    #[error("author connection failed")]
    Connect,
    /// Some request bytes may have arrived.
    #[error("author request write failed")]
    Write,
    /// The response head was invalid, incomplete, or late.
    #[error("author response head failed")]
    Head,
    /// Body framing, size, coding, or deadline failed after the request.
    #[error("author response body failed")]
    Body,
    /// An established event stream failed its complete-item heartbeat deadline.
    #[error("author event stream heartbeat deadline failed")]
    EventHeartbeat,
}

impl FiniteHttpFailure {
    /// Only failures before writing can establish nonexecution for a POST.
    #[must_use]
    pub fn request_may_have_reached_author(self) -> bool {
        matches!(self, Self::Write | Self::Head | Self::Body | Self::EventHeartbeat)
    }
}

/// Complete validated response and elapsed time from before connection work.
pub struct FiniteHttpReceipt {
    /// The response accepted by the common transport gate.
    pub response: SelectedAuthorFiniteResponse,
    /// Conservative elapsed time used to reduce advertised retention.
    pub elapsed_milliseconds: u64,
}

impl core::fmt::Debug for FiniteHttpReceipt {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("FiniteHttpReceipt([redacted])")
    }
}

/// Proof that a selected-author artifact passed framing, length and digest
/// checks. No receipt is returned for a partial or sink-refused transfer.
#[derive(Debug)]
pub struct ArtifactHttpReceipt {
    byte_length: u64,
    elapsed_milliseconds: u64,
}

impl ArtifactHttpReceipt {
    /// Constructed only by transport paths after exact framing/length/digest checks.
    pub(crate) fn verified(byte_length: u64, elapsed_milliseconds: u64) -> Self {
        Self { byte_length, elapsed_milliseconds }
    }
    /// Exact identity-coded content length.
    pub fn byte_length(&self) -> u64 {
        self.byte_length
    }
    /// Conservative elapsed time from before connection work.
    pub fn elapsed_milliseconds(&self) -> u64 {
        self.elapsed_milliseconds
    }
}

/// A fully framed artifact exchange, never an inference from status alone.
#[derive(Debug)]
pub enum ArtifactHttpOutcome {
    /// A completely framed, bounded 401; no artifact bytes reached the sink.
    Unauthorized,
    /// Verified bytes were streamed to the caller's private sink.
    Transferred(ArtifactHttpReceipt),
    /// A bounded identity-checked 404/410 body, with conservative elapsed time.
    Unavailable {
        /// Validated identity-bearing refusal.
        evidence: crate::artifact_download::ValidatedArtifactUnavailable,
        /// Time since before connection work.
        elapsed_milliseconds: u64,
    },
}

impl SelectedAuthorTransport {
    /// Streams a successful artifact response into private caller-owned staging.
    /// The caller validates the command/slot manifest and reserves capacity
    /// before calling. Sink writes must be bounded and must not publish content.
    /// Non-200 responses remain refusals, never proof of artifact retirement.
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
            ArtifactHttpOutcome::Unavailable { .. } | ArtifactHttpOutcome::Unauthorized => Err(FiniteHttpFailure::Head),
        }
    }

    /// Streams successful bytes or validates a closed unavailable response for
    /// the independently derived artifact identifier. Neither branch publishes.
    pub async fn artifact_http1(
        &self,
        identity: &slingshot_domain::operation_executor::ExecutionIdentity,
        submission: &crate::command_submission::Submission,
        expected: &crate::artifact_download::ExpectedArtifact,
        artifact_identifier: &str,
        authentication: &RequestAuthentication,
        sink: impl FnMut(&[u8]) -> Result<(), FiniteHttpFailure>,
    ) -> Result<ArtifactHttpOutcome, FiniteHttpFailure> {
        if artifact_identifier.is_empty() || artifact_identifier.len() > 128 {
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
            || expected.artifact_digest.len() != 64
            || !expected
                .artifact_digest
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || expected.byte_length
                > slingshot_domain::daemon_runtime_contract::DaemonRuntimeContract::embedded()
                    .formula("maximum_individual_artifact_bytes")
        {
            return Err(FiniteHttpFailure::Request);
        }
        let request = encode_request(
            self,
            Method::GET,
            &[
                "bin",
                "slingshot-agent",
                "operations",
                &submission.operation.agent_operation_identifier,
                "artifacts",
                &expected.artifact_slot,
            ],
            &[],
            authentication,
            &HeaderMap::new(),
            b"",
        )?;
        let started = Instant::now();
        let stream = self.connect().await.map_err(|_| FiniteHttpFailure::Connect)?;
        Self::artifact_http1_on_stream(stream, &request, started, submission, expected,
            artifact_identifier, sink).await
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
        if status != 200 {
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
                    &mut stream,
                    framing,
                    limit,
                    Duration::from_millis(deadlines.finite_idle_milliseconds),
                ),
            )
            .await
            .map_err(|_| FiniteHttpFailure::Body)??;
            if status == 401 { return Ok(ArtifactHttpOutcome::Unauthorized); }
            let evidence = crate::artifact_download::decode_artifact_unavailable(
                status,
                &body,
                submission,
                artifact_identifier,
                &expected.artifact_slot,
            )
            .map_err(|_| FiniteHttpFailure::Body)?;
            return Ok(ArtifactHttpOutcome::Unavailable {
                evidence,
                elapsed_milliseconds: u64::try_from(
                    started.elapsed().as_nanos().div_ceil(1_000_000),
                )
                .unwrap_or(u64::MAX),
            });
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
        // A synchronous staging sink cannot be preempted by Tokio's timeout.
        // Refuse an overdue transfer even if its final poll completed ready.
        if body_started.elapsed() >= total
            || received != expected.byte_length
            || digest != expected.artifact_digest
        {
            return Err(FiniteHttpFailure::Body);
        }
        Ok(ArtifactHttpOutcome::Transferred(ArtifactHttpReceipt {
            byte_length: received,
            elapsed_milliseconds: u64::try_from(started.elapsed().as_nanos().div_ceil(1_000_000))
                .unwrap_or(u64::MAX),
        }))
    }

    /// Sends one finite request below the selected author. The authentication
    /// value is borrowed for serialization and never included in diagnostics.
    /// Caller fields cannot override transport framing or authentication.
    pub async fn finite_http1(
        &self,
        method: Method,
        segments: &[&str],
        authentication: &RequestAuthentication,
        fields: &HeaderMap,
        body: &[u8],
    ) -> Result<FiniteHttpReceipt, FiniteHttpFailure> {
        self.finite_http1_query(method, segments, &[], authentication, fields, body).await
    }

    /// Sends a bounded, ordered query under the selected context prefix.
    /// Values are encoded from UTF-8 exactly once; no caller-supplied URL is accepted.
    pub async fn finite_http1_query(
        &self,
        method: Method,
        segments: &[&str],
        query: &[(&str, &str)],
        authentication: &RequestAuthentication,
        fields: &HeaderMap,
        body: &[u8],
    ) -> Result<FiniteHttpReceipt, FiniteHttpFailure> {
        let request = encode_request(self, method, segments, query, authentication, fields, body)?;
        let started = Instant::now();
        let stream = self.connect().await.map_err(|_| FiniteHttpFailure::Connect)?;
        Self::finite_http1_on_stream(stream, &request, started).await
    }

    /// Sends one finite request on the original negotiated connection. Both
    /// possible encodings are checked before connecting; the selected codec
    /// must be valid before any HTTP bytes are sent. An unselected codec's
    /// encoding limit does not constrain the selected one. No retry occurs.
    pub async fn finite_negotiated_query(
        &self,
        method: Method,
        segments: &[&str],
        query: &[(&str, &str)],
        authentication: &RequestAuthentication,
        fields: &HeaderMap,
        body: &[u8],
    ) -> Result<FiniteHttpReceipt, FiniteHttpFailure> {
        let http1 = encode_request(self, method.clone(), segments, query, authentication, fields, body);
        let http2 = self.encode_http2_request_head(method, segments, query, authentication, fields, body);
        if http1.is_err() && http2.is_err() {
            return Err(FiniteHttpFailure::Request);
        }
        let started = Instant::now();
        let (protocol, mut stream) = self.connect_negotiated().await
            .map_err(|_| FiniteHttpFailure::Connect)?.into_parts();
        if protocol == crate::selected_author_transport::SelectedHttpProtocol::Http1 {
            return Self::finite_http1_on_stream(stream, &http1?, started).await;
        }
        let http2 = http2?;
        let deadlines = ExchangeDeadlines::embedded();
        let negotiated = crate::selected_author_http2_handshake::negotiate(
            &mut stream, Duration::from_millis(deadlines.response_header_milliseconds),
        ).await?;
        let response = crate::selected_author_http2::drive(
            stream, negotiated, http2.frames(), body, deadlines,
        ).await?;
        Ok(FiniteHttpReceipt {
            response,
            elapsed_milliseconds: u64::try_from(started.elapsed().as_nanos().div_ceil(1_000_000))
                .unwrap_or(u64::MAX),
        })
    }

    async fn finite_http1_on_stream(
        mut stream: SelectedAuthorStream,
        request: &[u8],
        started: Instant,
    ) -> Result<FiniteHttpReceipt, FiniteHttpFailure> {
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
        let response = Response::builder()
            .status(status)
            .version(Version::HTTP_11)
            .body(Vec::<u8>::new())
            .map_err(|_| FiniteHttpFailure::Head)?;
        let (mut parts, _) = response.into_parts();
        parts.headers = headers;
        // Validate policy before allocating or reading the body.
        validate_collected_finite_response(CollectedFiniteResponse {
            response: Response::from_parts(parts.clone(), Vec::new()),
            framing_ambiguous: false,
            trailer_section_present: false,
            trailing_bytes: false,
        })
        .map_err(|_| FiniteHttpFailure::Head)?;
        let limit =
            AuthorAgentTransportContract::embedded().limit("maximum_finite_response_body_bytes");
        let body = timeout(
            Duration::from_millis(deadlines.finite_total_milliseconds),
            read_framed_body(
                &mut stream,
                framing,
                limit,
                Duration::from_millis(deadlines.finite_idle_milliseconds),
            ),
        )
        .await
        .map_err(|_| FiniteHttpFailure::Body)??;
        let response = validate_collected_finite_response(CollectedFiniteResponse {
            response: Response::from_parts(parts, body),
            framing_ambiguous: false,
            trailer_section_present: false,
            trailing_bytes: false,
        })
        .map_err(|_| FiniteHttpFailure::Body)?;
        // Round up: rounding down would overstate remaining retention.
        let nanos = started.elapsed().as_nanos();
        let elapsed_milliseconds = u64::try_from(nanos.div_ceil(1_000_000)).unwrap_or(u64::MAX);
        Ok(FiniteHttpReceipt { response, elapsed_milliseconds })
    }
}

pub(crate) enum BodyFraming {
    Fixed(u64),
    Chunked,
}

pub(crate) async fn read_framed_body(
    stream: &mut (impl AsyncRead + Unpin),
    framing: BodyFraming,
    limit: u64,
    idle: Duration,
) -> Result<Vec<u8>, FiniteHttpFailure> {
    let mut body = Vec::new();
    match framing {
        BodyFraming::Fixed(length) => {
            read_body_part(stream, &mut body, length, limit, idle).await?
        }
        BodyFraming::Chunked => loop {
            let line = read_chunk_line(stream, idle).await?;
            let length = decode_chunk_size(&line)?;
            if length == 0 {
                if !read_chunk_line(stream, idle).await?.is_empty() {
                    return Err(FiniteHttpFailure::Body);
                }
                break;
            }
            read_body_part(stream, &mut body, length, limit, idle).await?;
            if !read_chunk_line(stream, idle).await?.is_empty() {
                return Err(FiniteHttpFailure::Body);
            }
        },
    }
    let mut extra = [0];
    if timeout(idle, stream.read(&mut extra))
        .await
        .map_err(|_| FiniteHttpFailure::Body)?
        .map_err(|_| FiniteHttpFailure::Body)?
        != 0
    {
        return Err(FiniteHttpFailure::Body);
    }
    Ok(body)
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
    let mut buffer = [0_u8; 8192];
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

async fn read_body_part(
    stream: &mut (impl AsyncRead + Unpin),
    body: &mut Vec<u8>,
    length: u64,
    limit: u64,
    idle: Duration,
) -> Result<(), FiniteHttpFailure> {
    let end = (body.len() as u64)
        .checked_add(length)
        .filter(|n| *n <= limit)
        .ok_or(FiniteHttpFailure::Body)?;
    let mut position = body.len();
    body.resize(usize::try_from(end).map_err(|_| FiniteHttpFailure::Body)?, 0);
    while position < body.len() {
        let count = timeout(idle, stream.read(&mut body[position..]))
            .await
            .map_err(|_| FiniteHttpFailure::Body)?
            .map_err(|_| FiniteHttpFailure::Body)?;
        if count == 0 {
            return Err(FiniteHttpFailure::Body);
        }
        position += count;
    }
    Ok(())
}

/// Validates RFC 9112 section 7.1.1 extensions without collecting or exposing
/// their metadata. The caller bounds the entire line, including extensions.
pub(crate) fn decode_chunk_size(line: &[u8]) -> Result<u64, FiniteHttpFailure> {
    let digits = line.iter().take_while(|byte| byte.is_ascii_hexdigit()).count();
    if digits == 0 {
        return Err(FiniteHttpFailure::Body);
    }
    let length = u64::from_str_radix(
        std::str::from_utf8(&line[..digits]).map_err(|_| FiniteHttpFailure::Body)?,
        16,
    )
    .map_err(|_| FiniteHttpFailure::Body)?;
    let mut rest = &line[digits..];
    let token = |byte: &u8| byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(byte);
    while !rest.is_empty() {
        rest = skip_chunk_whitespace(rest);
        rest = rest.strip_prefix(b";").ok_or(FiniteHttpFailure::Body)?;
        rest = skip_chunk_whitespace(rest);
        let name = rest.iter().take_while(|byte| token(byte)).count();
        if name == 0 {
            return Err(FiniteHttpFailure::Body);
        }
        rest = &rest[name..];
        let after_whitespace = skip_chunk_whitespace(rest);
        if let Some(value) = after_whitespace.strip_prefix(b"=") {
            rest = skip_chunk_whitespace(value);
            if let Some(quoted) = rest.strip_prefix(b"\"") {
                rest = quoted;
                loop {
                    let (&byte, remaining) = rest.split_first().ok_or(FiniteHttpFailure::Body)?;
                    rest = remaining;
                    match byte {
                        b'"' => break,
                        b'\\' => {
                            let (&escaped, remaining) =
                                rest.split_first().ok_or(FiniteHttpFailure::Body)?;
                            if !matches!(escaped, b'\t' | b' '..=b'~' | 0x80..=0xff) {
                                return Err(FiniteHttpFailure::Body);
                            }
                            rest = remaining;
                        }
                        b'\t' | b' ' | b'!' | b'#'..=b'[' | b']'..=b'~' | 0x80..=0xff => {}
                        _ => return Err(FiniteHttpFailure::Body),
                    }
                }
            } else {
                let value = rest.iter().take_while(|byte| token(byte)).count();
                if value == 0 {
                    return Err(FiniteHttpFailure::Body);
                }
                rest = &rest[value..];
            }
        }
    }
    Ok(length)
}

fn skip_chunk_whitespace(bytes: &[u8]) -> &[u8] {
    &bytes[bytes.iter().take_while(|byte| matches!(byte, b' ' | b'\t')).count()..]
}

#[cfg(test)]
mod chunk_tests {
    use super::decode_chunk_size;

    #[test]
    fn extensions_are_validated_and_ignored_without_changing_the_size() {
        for line in [
            &b"a"[..],
            b"0A;flag",
            b"a ; name = value ;next",
            b"a;empty=\"\"",
            b"a;quoted=\"semi;colon=equals\\\"quote\\\\slash\"",
            b"a;x=\"\t\xff\"",
        ] {
            assert_eq!(decode_chunk_size(line).unwrap(), 10, "{line:?}");
        }
        assert_eq!(decode_chunk_size(b"000;final=yes").unwrap(), 0);
        assert_eq!(decode_chunk_size(b"FFFFFFFFFFFFFFFF;flag").unwrap(), u64::MAX);
    }

    #[test]
    fn malformed_extensions_and_overflow_are_not_framing_evidence() {
        for line in [
            &b""[..],
            b" a",
            b"+a",
            b"a ",
            b"a;",
            b"a;;x",
            b"a;=x",
            b"a;x=",
            b"a;x=\"unterminated",
            b"a;x=\"bad\\",
            b"a;x=\"bad\r\"",
            b"a;x=\"bad\\\n\"",
            b"a;x=\"ok\"surplus",
            b"a;x=y ",
            b"a;x=y,z",
            b"a;\xff=x",
            b"a;x=\xff",
            b"10000000000000000;flag",
            b"a;name\x00",
            b"a;x=\"\x7f\"",
        ] {
            assert!(decode_chunk_size(line).is_err(), "{line:?}");
        }
    }
}

pub(crate) async fn read_chunk_line(
    stream: &mut (impl AsyncRead + Unpin),
    idle: Duration,
) -> Result<Vec<u8>, FiniteHttpFailure> {
    let mut line = Vec::new();
    loop {
        if line.len() as u64 >= HeadBounds::embedded().field_bytes {
            return Err(FiniteHttpFailure::Body);
        }
        let byte = timeout(idle, stream.read_u8())
            .await
            .map_err(|_| FiniteHttpFailure::Body)?
            .map_err(|_| FiniteHttpFailure::Body)?;
        line.push(byte);
        if line.ends_with(b"\r\n") {
            line.truncate(line.len() - 2);
            return Ok(line);
        }
        if byte == b'\n' {
            return Err(FiniteHttpFailure::Body);
        }
    }
}

pub(crate) fn prepare_request_uri(
    transport: &SelectedAuthorTransport,
    segments: &[&str],
    query: &[(&str, &str)],
    fields: &HeaderMap,
    body: &[u8],
) -> Result<Uri, FiniteHttpFailure> {
    let mut endpoint = transport.endpoint(segments);
    let limit = AuthorAgentTransportContract::embedded().limit("maximum_route_query_bytes");
    let mut encoded = String::new();
    for (index, (name, value)) in query.iter().enumerate() {
        if name.is_empty()
            || query[..index].iter().any(|(previous, _)| previous == name)
            || (name.len() as u64).saturating_add(value.len() as u64) > limit
        {
            return Err(FiniteHttpFailure::Request);
        }
        if index != 0 {
            encoded.push('&');
        }
        encoded.push_str(&crate::job_snapshot_reconciliation::encoded_once(name));
        encoded.push('=');
        encoded.push_str(&crate::job_snapshot_reconciliation::encoded_once(value));
        if encoded.len() as u64 > limit {
            return Err(FiniteHttpFailure::Request);
        }
    }
    if !encoded.is_empty() {
        endpoint.push('?');
        endpoint.push_str(&encoded);
    }
    let uri: Uri = endpoint.parse().map_err(|_| FiniteHttpFailure::Request)?;
    uri.authority().ok_or(FiniteHttpFailure::Request)?;
    uri.path_and_query().ok_or(FiniteHttpFailure::Request)?;
    let limit =
        AuthorAgentTransportContract::embedded().limit("maximum_finite_response_body_bytes");
    if body.len() as u64 > limit {
        return Err(FiniteHttpFailure::Request);
    }
    for name in fields.keys() {
        if [
            "host",
            "connection",
            "accept-encoding",
            "content-length",
            "transfer-encoding",
            "authorization",
            "expect",
            "upgrade",
            "trailer",
            "proxy-connection",
            "keep-alive",
            "te",
        ]
        .contains(&name.as_str())
        {
            return Err(FiniteHttpFailure::Request);
        }
    }
    Ok(uri)
}

pub(crate) fn encode_request(
    transport: &SelectedAuthorTransport,
    method: Method,
    segments: &[&str],
    query: &[(&str, &str)],
    authentication: &RequestAuthentication,
    fields: &HeaderMap,
    body: &[u8],
) -> Result<Vec<u8>, FiniteHttpFailure> {
    transport.require_authentication(authentication).map_err(|_| FiniteHttpFailure::Request)?;
    let uri = prepare_request_uri(transport, segments, query, fields, body)?;
    let authority = uri.authority().ok_or(FiniteHttpFailure::Request)?;
    let mut request = format!("{} {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nAccept-Encoding: identity\r\nContent-Length: {}\r\n",
        method, uri.path_and_query().ok_or(FiniteHttpFailure::Request)?, authority, body.len()).into_bytes();
    for (name, value) in fields {
        request.extend_from_slice(name.as_str().as_bytes());
        request.extend_from_slice(b": ");
        request.extend_from_slice(value.as_bytes());
        request.extend_from_slice(b"\r\n");
    }
    authentication.lend_value_bytes(|value| {
        HeaderValue::from_bytes(value).map_err(|_| FiniteHttpFailure::Request)?;
        request.extend_from_slice(b"Authorization: ");
        request.extend_from_slice(value);
        Ok::<(), FiniteHttpFailure>(())
    })?;
    request.extend_from_slice(b"\r\n\r\n");
    request.extend_from_slice(body);
    Ok(request)
}

pub(crate) async fn read_head(
    stream: &mut (impl AsyncRead + Unpin),
) -> Result<(u16, HeaderMap, BodyFraming), FiniteHttpFailure> {
    let bounds = HeadBounds::embedded();
    let mut charged = 0_u64;
    let status = read_line(stream, bounds.head_bytes, &mut charged, bounds.head_bytes).await?;
    let text = std::str::from_utf8(&status).map_err(|_| FiniteHttpFailure::Head)?;
    let mut words = text.splitn(3, ' ');
    if words.next() != Some("HTTP/1.1") {
        return Err(FiniteHttpFailure::Head);
    }
    let code = words.next().ok_or(FiniteHttpFailure::Head)?;
    if code.len() != 3 || !code.bytes().all(|b| b.is_ascii_digit()) {
        return Err(FiniteHttpFailure::Head);
    }
    let status: u16 = code.parse().map_err(|_| FiniteHttpFailure::Head)?;
    if !(200..600).contains(&status) || (300..400).contains(&status) {
        return Err(FiniteHttpFailure::Head);
    }
    let mut headers = HeaderMap::new();
    let mut reader = HeadReader::new(bounds);
    loop {
        let line = read_field_line(stream, bounds, reader.fields(), &mut charged).await?;
        if line.is_empty() {
            break;
        }
        let colon = line.iter().position(|b| *b == b':').ok_or(FiniteHttpFailure::Head)?;
        let name = HeaderName::from_bytes(&line[..colon]).map_err(|_| FiniteHttpFailure::Head)?;
        let value = std::str::from_utf8(&line[colon + 1..])
            .map_err(|_| FiniteHttpFailure::Head)?
            .trim_matches([' ', '\t']);
        reader.read_field(name.as_str(), value).map_err(|_| FiniteHttpFailure::Head)?;
        headers.append(name, HeaderValue::from_str(value).map_err(|_| FiniteHttpFailure::Head)?);
    }
    if headers.contains_key("upgrade") {
        return Err(FiniteHttpFailure::Head);
    }
    let lengths: Vec<_> = headers.get_all("content-length").iter().collect();
    let transfers: Vec<_> = headers.get_all("transfer-encoding").iter().collect();
    if !transfers.is_empty() {
        if !lengths.is_empty()
            || transfers.len() != 1
            || !transfers[0].as_bytes().eq_ignore_ascii_case(b"chunked")
        {
            return Err(FiniteHttpFailure::Head);
        }
        return Ok((status, headers, BodyFraming::Chunked));
    }
    let [length] = lengths.as_slice() else {
        return Err(FiniteHttpFailure::Head);
    };
    let text = length.to_str().map_err(|_| FiniteHttpFailure::Head)?;
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(FiniteHttpFailure::Head);
    }
    let length = text.parse().map_err(|_| FiniteHttpFailure::Head)?;
    Ok((status, headers, BodyFraming::Fixed(length)))
}

async fn read_line(
    stream: &mut (impl AsyncRead + Unpin),
    limit: u64,
    charged: &mut u64,
    total: u64,
) -> Result<Vec<u8>, FiniteHttpFailure> {
    let mut line = Vec::new();
    loop {
        if *charged >= total || line.len() as u64 >= limit {
            return Err(FiniteHttpFailure::Head);
        }
        let byte = stream.read_u8().await.map_err(|_| FiniteHttpFailure::Head)?;
        *charged += 1;
        line.push(byte);
        if line.ends_with(b"\r\n") {
            line.truncate(line.len() - 2);
            return Ok(line);
        }
        if byte == b'\n' {
            return Err(FiniteHttpFailure::Head);
        }
    }
}

/// Charges raw head bytes independently from the decoded name-plus-value
/// limit. Optional surrounding whitespace and delimiters are raw bytes only;
/// whitespace inside a value is charged when the next value byte arrives.
/// No oversized decoded field or extra field is collected before refusal.
async fn read_field_line(
    stream: &mut (impl AsyncRead + Unpin),
    bounds: HeadBounds,
    fields: usize,
    charged: &mut u64,
) -> Result<Vec<u8>, FiniteHttpFailure> {
    let mut line = Vec::new();
    let mut in_value = false;
    let mut value_started = false;
    let mut decoded = 0_u64;
    let mut pending_whitespace = 0_u64;
    let mut carriage_return = false;
    loop {
        if *charged >= bounds.head_bytes {
            return Err(FiniteHttpFailure::Head);
        }
        let byte = stream.read_u8().await.map_err(|_| FiniteHttpFailure::Head)?;
        *charged += 1;
        if carriage_return {
            return if byte == b'\n' { Ok(line) } else { Err(FiniteHttpFailure::Head) };
        }
        if byte == b'\r' {
            carriage_return = true;
            continue;
        }
        if byte == b'\n' || fields as u64 >= bounds.field_count {
            return Err(FiniteHttpFailure::Head);
        }
        if !in_value && byte == b':' {
            in_value = true;
        } else if in_value && matches!(byte, b' ' | b'\t') {
            if value_started {
                pending_whitespace = pending_whitespace.checked_add(1).ok_or(FiniteHttpFailure::Head)?;
            }
        } else {
            decoded = decoded.checked_add(pending_whitespace)
                .and_then(|count| count.checked_add(1))
                .filter(|count| *count <= bounds.field_bytes)
                .ok_or(FiniteHttpFailure::Head)?;
            pending_whitespace = 0;
            value_started |= in_value;
        }
        line.push(byte);
    }
}

#[cfg(test)]
mod response_field_tests {
    use super::*;

    #[tokio::test]
    async fn decoded_fields_and_raw_heads_have_independent_incremental_limits() {
        // Refusal cases intentionally omit the line ending. The peer waits
        // for client closure, proving rejection does not wait for more bytes.
        for (wire, fields, raw_limit, expected) in [
            ("x:1234567\r\n", 0, 64, Some("x:1234567")),
            ("x: \t1234567 \t\r\n", 0, 64, Some("x: \t1234567 \t")),
            ("x:12  345\r\n", 0, 64, Some("x:12  345")),
            ("abcdefgh: \t\r\n", 0, 64, Some("abcdefgh: \t")),
            ("x:12345678", 0, 64, None),
            ("x:12  3456", 0, 64, None),
            ("abcdefghi", 0, 64, None),
            ("x:1234567\r\n", 0, 11, Some("x:1234567")),
            ("x:1234567\r", 0, 10, None),
            ("x:          ", 0, 12, None),
            ("x", 2, 64, None),
            ("\r\n", 2, 2, Some("")),
            ("x:1\n", 0, 64, None),
            ("x:1\rx", 0, 64, None),
        ] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let peer = async {
                let (mut socket, _) = listener.accept().await.unwrap();
                socket.write_all(wire.as_bytes()).await.unwrap();
                let mut byte = [0];
                // A reset is also a closed client on platforms where unread
                // server bytes remain after an early boundary refusal.
                assert!(matches!(socket.read(&mut byte).await, Ok(0) | Err(_)));
            };
            let client = async {
                let socket = tokio::net::TcpStream::connect(address).await.unwrap();
                let mut stream = SelectedAuthorStream::Cleartext(socket);
                let mut charged = 0;
                let result = read_field_line(&mut stream, HeadBounds {
                    field_bytes: 8, field_count: 2, head_bytes: raw_limit,
                }, fields, &mut charged).await;
                match expected {
                    Some(line) => assert_eq!(result.unwrap(), line.as_bytes(), "{wire:?}"),
                    None => assert!(matches!(result, Err(FiniteHttpFailure::Head)), "{wire:?}"),
                }
                assert!(charged <= raw_limit);
            };
            timeout(Duration::from_secs(5), async { tokio::join!(client, peer) }).await.unwrap();
        }
    }
}
