//! Bounded, single-request IMS HTTP/1.1 codec on an already authenticated socket.

use super::{
    async_identity_management_exchange::IdentityManagementReceipt,
    identity_management_exchange::{
        DecodedHead, DecodedResponse, ExchangeFailure, MonotonicClock, accept_media,
        identity_management_endpoint,
    },
};
use slingshot_domain::profile_authentication_contract::{
    ConfigurationFailureCode as Code, ProfileAuthenticationContract,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    time::{Duration, Instant, timeout_at},
};

/// Sends only the fixed IMS POST, owns the stream through complete EOF, and
/// drops it on failure/cancellation. The caller additionally bounds connection
/// setup and this entire future with the manifest whole-exchange deadline.
pub async fn exchange_http1<S: AsyncRead + AsyncWrite + Unpin>(
    mut stream: S,
    body: &[u8],
    clock: &(dyn MonotonicClock + Sync),
) -> Result<IdentityManagementReceipt, ExchangeFailure> {
    let limits = &ProfileAuthenticationContract::embedded().limits;
    if body.len() as u64 > limits.maximum_identity_management_request_body_bytes {
        return Err(fail(Code::IdentityManagementResponseHeadLimitExceeded));
    }
    let endpoint = url::Url::parse(&identity_management_endpoint()).map_err(|_| malformed())?;
    let host = endpoint.host_str().ok_or_else(malformed)?;
    let request = format!(
        "POST {} HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/x-www-form-urlencoded\r\nAccept: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        endpoint.path(),
        body.len()
    );
    let write_deadline = Instant::now()
        + Duration::from_millis(limits.identity_management_request_write_timeout_milliseconds);
    let anchor = clock.reading_milliseconds();
    timeout_at(write_deadline, async {
        stream.write_all(request.as_bytes()).await?;
        stream.write_all(body).await?;
        stream.flush().await
    })
    .await
    .map_err(|_| fail(Code::IdentityManagementRequestWriteTimeout))?
    .map_err(|_| malformed())?;
    if Instant::now() >= write_deadline {
        return Err(fail(Code::IdentityManagementRequestWriteTimeout));
    }
    let deadline = Instant::now()
        + Duration::from_millis(limits.identity_management_response_header_timeout_milliseconds);
    let head = read_head(&mut stream, deadline).await?;
    let framing = framing(&head.fields)?;
    if (100..200).contains(&head.status) {
        return Err(fail(Code::IdentityManagementResponseStatusRejected));
    }
    if (300..400).contains(&head.status) {
        return Err(fail(Code::IdentityManagementRedirectRefused));
    }
    if u64::from(head.status) != limits.identity_management_response_success_status {
        return Err(fail(Code::IdentityManagementResponseStatusRejected));
    }
    if head.fields.iter().any(|(name, _)| name == "trailer") {
        return Err(fail(Code::IdentityManagementResponseTrailerRejected));
    }
    accept_media(&head.fields)?;
    let total = Instant::now()
        + Duration::from_millis(
            limits.identity_management_response_body_total_timeout_milliseconds,
        );
    let mut reader = BodyReader { stream: &mut stream, total };
    let maximum = usize::try_from(limits.maximum_identity_management_response_body_bytes)
        .map_err(|_| malformed())?;
    let mut collected = SensitiveBody(Vec::with_capacity(maximum));
    match framing {
        Framing::Length(length) => {
            for _ in 0..length {
                push(&mut collected.0, reader.required().await?, maximum)?;
            }
        }
        Framing::Chunked => loop {
            let line = reader
                .line(limits.maximum_identity_management_response_header_bytes as usize)
                .await?;
            let size =
                crate::selected_author_http::decode_chunk_size(&line).map_err(|_| malformed())?;
            if size == 0 {
                let mut fields = Vec::new();
                let mut charge = limits.identity_management_response_trailer_charge_bytes;
                loop {
                    let trailer = reader
                        .line(limits.maximum_identity_management_response_header_bytes as usize + 2)
                        .await?;
                    if trailer.is_empty() {
                        break;
                    }
                    accept_field(&trailer, &mut fields, &mut charge)?;
                }
                // IMS refuses the trailer section itself, including the empty
                // section following the last chunk. Do not erase its presence.
                return Err(fail(Code::IdentityManagementResponseTrailerRejected));
            }
            for _ in 0..size {
                push(&mut collected.0, reader.required().await?, maximum)?;
            }
            if reader.required().await? != b'\r' || reader.required().await? != b'\n' {
                return Err(malformed());
            }
        },
        Framing::Close => {
            while let Some(byte) = reader.byte().await? {
                push(&mut collected.0, byte, maximum)?;
            }
            return Ok(receipt(head, std::mem::take(&mut collected.0), anchor, clock));
        }
    }
    if reader.byte().await?.is_some() {
        return Err(malformed());
    }
    Ok(receipt(head, std::mem::take(&mut collected.0), anchor, clock))
}

struct SensitiveBody(Vec<u8>);
impl Drop for SensitiveBody {
    fn drop(&mut self) {
        let _secret =
            slingshot_domain::secret_value::SecretValue::from_bytes(std::mem::take(&mut self.0));
    }
}

fn receipt(
    head: DecodedHead,
    body: Vec<u8>,
    anchor: u64,
    clock: &(dyn MonotonicClock + Sync),
) -> IdentityManagementReceipt {
    IdentityManagementReceipt::new(
        DecodedResponse { informational: Vec::new(), head, body, trailer: None },
        anchor,
        clock.reading_milliseconds(),
    )
}
fn push(body: &mut Vec<u8>, byte: u8, maximum: usize) -> Result<(), ExchangeFailure> {
    if body.len() == maximum {
        return Err(fail(Code::IdentityManagementResponseBodyLimitExceeded));
    }
    body.push(byte);
    Ok(())
}

async fn read_head<S: AsyncRead + Unpin>(
    stream: &mut S,
    deadline: Instant,
) -> Result<DecodedHead, ExchangeFailure> {
    let limits = &ProfileAuthenticationContract::embedded().limits;
    let maximum = usize::try_from(limits.maximum_identity_management_response_header_bytes)
        .map_err(|_| malformed())?;
    let status = head_line(stream, deadline, maximum).await?;
    if status.len() < 13
        || &status[..9] != b"HTTP/1.1 "
        || status[12..].iter().any(|byte| *byte < 32 || *byte == 127)
        || status[12] != b' '
        || !status[9..12].iter().all(u8::is_ascii_digit)
    {
        return Err(malformed());
    }
    let status = u16::from(status[9] - b'0') * 100
        + u16::from(status[10] - b'0') * 10
        + u16::from(status[11] - b'0');
    if !(100..600).contains(&status) {
        return Err(malformed());
    }
    let mut fields = Vec::new();
    let mut charge = limits.identity_management_response_head_status_charge_bytes;
    loop {
        let line = head_line(stream, deadline, maximum.saturating_add(2)).await?;
        if line.is_empty() {
            break;
        }
        accept_field(&line, &mut fields, &mut charge)?;
    }
    Ok(DecodedHead { status, fields })
}

fn accept_field(
    line: &[u8],
    fields: &mut Vec<(String, String)>,
    charge: &mut u64,
) -> Result<(), ExchangeFailure> {
    let limits = &ProfileAuthenticationContract::embedded().limits;
    let colon = line.iter().position(|byte| *byte == b':').ok_or_else(malformed)?;
    let name = http::HeaderName::from_bytes(&line[..colon]).map_err(|_| malformed())?;
    let mut value = &line[colon + 1..];
    while matches!(value.first(), Some(b' ' | b'\t')) {
        value = &value[1..];
    }
    while matches!(value.last(), Some(b' ' | b'\t')) {
        value = &value[..value.len() - 1];
    }
    if value.iter().any(|byte| *byte < 32 || *byte == 127) {
        return Err(malformed());
    }
    let field =
        (name.as_str().len() as u64).checked_add(value.len() as u64).ok_or_else(head_limit)?;
    *charge = charge
        .checked_add(field)
        .and_then(|n| n.checked_add(limits.identity_management_response_field_charge_bytes))
        .ok_or_else(head_limit)?;
    if field > limits.maximum_identity_management_response_header_bytes
        || *charge > limits.maximum_identity_management_response_head_bytes
        || fields.len() as u64 >= limits.maximum_identity_management_response_header_count
    {
        return Err(head_limit());
    }
    let value = String::from_utf8(value.to_vec()).map_err(|_| malformed())?;
    fields.push((name.as_str().to_owned(), value));
    Ok(())
}
async fn head_line<S: AsyncRead + Unpin>(
    stream: &mut S,
    deadline: Instant,
    maximum: usize,
) -> Result<Vec<u8>, ExchangeFailure> {
    let mut line = Vec::new();
    loop {
        let mut byte = [0];
        if Instant::now() >= deadline {
            return Err(fail(Code::IdentityManagementResponseHeaderTimeout));
        }
        let count = timeout_at(deadline, stream.read(&mut byte))
            .await
            .map_err(|_| fail(Code::IdentityManagementResponseHeaderTimeout))?
            .map_err(|_| malformed())?;
        if Instant::now() >= deadline {
            return Err(fail(Code::IdentityManagementResponseHeaderTimeout));
        }
        if count == 0 {
            return Err(malformed());
        }
        if byte[0] == b'\n' {
            if line.pop() != Some(b'\r') {
                return Err(malformed());
            }
            return Ok(line);
        }
        if line.len() >= maximum.saturating_add(1) {
            return Err(head_limit());
        }
        line.push(byte[0]);
    }
}
enum Framing {
    Length(u64),
    Chunked,
    Close,
}
fn framing(fields: &[(String, String)]) -> Result<Framing, ExchangeFailure> {
    let mut length = None;
    let mut chunked = false;
    for (name, value) in fields {
        match name.as_str() {
            "content-length" => {
                let value = value.trim();
                if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
                    return Err(malformed());
                }
                let parsed = value.parse::<u64>().map_err(|_| malformed())?;
                if length.is_some_and(|length| length != parsed) {
                    return Err(malformed());
                }
                length = Some(parsed);
            }
            "transfer-encoding" => {
                if chunked || !value.trim().eq_ignore_ascii_case("chunked") {
                    return Err(malformed());
                }
                chunked = true;
            }
            _ => {}
        }
    }
    if chunked && length.is_some() {
        return Err(malformed());
    }
    Ok(if chunked {
        Framing::Chunked
    } else if let Some(length) = length {
        Framing::Length(length)
    } else {
        Framing::Close
    })
}
struct BodyReader<'a, S> {
    stream: &'a mut S,
    total: Instant,
}
impl<S: AsyncRead + Unpin> BodyReader<'_, S> {
    async fn byte(&mut self) -> Result<Option<u8>, ExchangeFailure> {
        let idle = Instant::now()
            + Duration::from_millis(
                ProfileAuthenticationContract::embedded()
                    .limits
                    .identity_management_response_body_idle_timeout_milliseconds,
            );
        let mut byte = [0];
        let deadline = self.total.min(idle);
        let elapsed = || {
            fail(if self.total <= idle {
                Code::IdentityManagementResponseBodyTotalTimeout
            } else {
                Code::IdentityManagementResponseBodyIdleTimeout
            })
        };
        if Instant::now() >= deadline {
            return Err(elapsed());
        }
        let count = timeout_at(deadline, self.stream.read(&mut byte))
            .await
            .map_err(|_| {
                fail(if self.total <= idle {
                    Code::IdentityManagementResponseBodyTotalTimeout
                } else {
                    Code::IdentityManagementResponseBodyIdleTimeout
                })
            })?
            .map_err(|_| malformed())?;
        if Instant::now() >= deadline {
            return Err(elapsed());
        }
        Ok((count != 0).then_some(byte[0]))
    }
    async fn required(&mut self) -> Result<u8, ExchangeFailure> {
        self.byte().await?.ok_or_else(malformed)
    }
    async fn line(&mut self, maximum: usize) -> Result<Vec<u8>, ExchangeFailure> {
        let mut line = Vec::new();
        loop {
            let byte = self.required().await?;
            if byte == b'\n' {
                if line.pop() != Some(b'\r') {
                    return Err(malformed());
                }
                return Ok(line);
            }
            if line.len() >= maximum.saturating_add(1) {
                return Err(head_limit());
            }
            line.push(byte);
        }
    }
}
fn fail(code: Code) -> ExchangeFailure {
    ExchangeFailure::new(code)
}
fn malformed() -> ExchangeFailure {
    fail(Code::IdentityManagementTransportFailed)
}
fn head_limit() -> ExchangeFailure {
    fail(Code::IdentityManagementResponseHeadLimitExceeded)
}

#[cfg(test)]
#[path = "identity_management_http1_tests.rs"]
mod tests;
