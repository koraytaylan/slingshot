//! Bounded HTTP/2 response frames for one request on stream 1.
//!
//! This is the wire gate before HPACK, not a complete HTTP/2 client. Callers
//! still need decoded-header limits, pseudo-header/status validation, flow
//! control, deadlines and route codecs before exposing any response evidence.
//! The client must advertise the default 16,384-byte maximum frame size and
//! disable server push. No server setting can enlarge our receiving limit.

use tokio::io::{AsyncRead, AsyncReadExt};

use crate::author_hypertext_transfer_protocol_policy::HeadBounds;

const MAXIMUM_FRAME_BYTES: usize = 16_384;

/// Opaque refusal; no peer-controlled header or payload bytes are retained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the author HTTP/2 response framing is invalid")]
pub struct ResponseFrameRefusal;

/// Clean transport EOF observed after a complete response stream. Only this
/// reader can construct the proof; it is consumed by finite completion.
#[derive(Debug)]
pub struct TransportEnd {
    _private: (),
}

impl TransportEnd {
    #[cfg(test)]
    pub(crate) fn for_test() -> Self {
        Self { _private: () }
    }
}

/// A bounded frame, or a clean final boundary rather than truncated framing.
#[derive(Debug)]
pub enum FrameRead {
    /// The caller must process every frame before asking for the next one.
    Frame(ResponseFrame),
    /// Complete HEADERS and END_STREAM preceded this clean transport EOF.
    End(TransportEnd),
}

/// One bounded wire frame, not yet authenticated command or response evidence.
pub struct ResponseFrame {
    /// Standard HTTP/2 frame type octet.
    pub kind: u8,
    /// Standard HTTP/2 flags octet.
    pub flags: u8,
    /// Stream identifier with the reserved bit removed.
    pub stream_identifier: u32,
    /// HEADERS payloads contain only HPACK bytes, without priority or padding.
    /// Other payloads retain their wire layout for their respective codecs.
    pub payload: Vec<u8>,
}

impl core::fmt::Debug for ResponseFrame {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("ResponseFrame([redacted])")
    }
}

impl Drop for ResponseFrame {
    fn drop(&mut self) {
        let _secret = slingshot_domain::secret_value::SecretValue::from_bytes(std::mem::take(
            &mut self.payload,
        ));
    }
}
struct SensitivePayload(Vec<u8>);
impl Drop for SensitivePayload {
    fn drop(&mut self) {
        let _secret =
            slingshot_domain::secret_value::SecretValue::from_bytes(std::mem::take(&mut self.0));
    }
}

/// One response's incremental encoded-header and frame-order accounting.
/// A failed or cancelled read poisons the reader; it cannot resume mid-frame.
#[derive(Debug)]
pub struct ResponseFrameReader {
    maximum_header_bytes: u64,
    encoded_header_bytes: u64,
    first_frame: bool,
    head_started: bool,
    continuation: bool,
    stream_ended: bool,
    stream_reset: bool,
    poisoned: bool,
    allow_trailer_section: bool,
    trailer_started: bool,
    header_limit_exceeded: bool,
}

impl Default for ResponseFrameReader {
    fn default() -> Self {
        Self::new()
    }
}

impl ResponseFrameReader {
    /// Uses the installed transport contract, not a server-selected limit.
    pub fn new() -> Self {
        Self::with_header_policy(HeadBounds::embedded().head_bytes, false)
    }

    /// Installs a route's compressed-section bound. Opting into one trailer
    /// section permits incremental accounting only; the route must still refuse
    /// trailers when its contract forbids them. Default author behavior is unchanged.
    pub fn with_header_policy(maximum_header_bytes: u64, allow_trailer_section: bool) -> Self {
        Self {
            maximum_header_bytes,
            encoded_header_bytes: 0,
            first_frame: true,
            head_started: false,
            continuation: false,
            stream_ended: false,
            stream_reset: false,
            poisoned: false,
            allow_trailer_section,
            trailer_started: false,
            header_limit_exceeded: false,
        }
    }

    /// Whether the first refusal was an encoded-section bound, before allocation.
    pub fn header_limit_exceeded(&self) -> bool {
        self.header_limit_exceeded
    }

    /// Reads one frame under the caller's enclosing transport deadline.
    /// Frame and encoded-header bounds are checked before payload allocation.
    pub async fn read(
        &mut self,
        stream: &mut (impl AsyncRead + Unpin),
    ) -> Result<ResponseFrame, ResponseFrameRefusal> {
        match self.read_next(stream).await? {
            FrameRead::Frame(frame) => Ok(frame),
            FrameRead::End(_) => Err(ResponseFrameRefusal),
        }
    }

    /// Distinguishes clean post-response EOF from early EOF, a partial frame,
    /// unfinished CONTINUATION sequence, and a failed/cancelled read. An end
    /// proof can be issued only once; the reader remains poisoned afterwards.
    pub async fn read_next(
        &mut self,
        stream: &mut (impl AsyncRead + Unpin),
    ) -> Result<FrameRead, ResponseFrameRefusal> {
        if self.poisoned {
            return Err(ResponseFrameRefusal);
        }
        self.poisoned = true;
        match self.read_frame(stream).await? {
            Some(frame) => {
                self.poisoned = false;
                Ok(FrameRead::Frame(frame))
            }
            None => Ok(FrameRead::End(TransportEnd { _private: () })),
        }
    }

    async fn read_frame(
        &mut self,
        stream: &mut (impl AsyncRead + Unpin),
    ) -> Result<Option<ResponseFrame>, ResponseFrameRefusal> {
        let mut head = [0; 9];
        let count = stream.read(&mut head[..1]).await.map_err(|_| ResponseFrameRefusal)?;
        if count == 0 {
            return if self.head_started
                && self.stream_ended
                && !self.continuation
                && !self.stream_reset
            {
                Ok(None)
            } else {
                Err(ResponseFrameRefusal)
            };
        }
        stream.read_exact(&mut head[1..]).await.map_err(|_| ResponseFrameRefusal)?;
        let length = usize::from(head[0]) << 16 | usize::from(head[1]) << 8 | usize::from(head[2]);
        let kind = head[3];
        let flags = head[4];
        let stream_identifier = u32::from_be_bytes(head[5..9].try_into().unwrap()) & 0x7fff_ffff;
        if length > MAXIMUM_FRAME_BYTES
            || (self.first_frame && (kind != 4 || flags & 1 != 0))
            || (self.continuation && (kind != 9 || stream_identifier != 1))
        {
            return Err(ResponseFrameRefusal);
        }
        self.first_frame = false;
        match kind {
            0 if stream_identifier != 1 || !self.head_started || self.stream_ended => {
                return Err(ResponseFrameRefusal);
            }
            1 if stream_identifier != 1
                || self.stream_ended
                || self.head_started
                    && (!self.allow_trailer_section || self.trailer_started || flags & 1 == 0) =>
            {
                return Err(ResponseFrameRefusal);
            }
            2 if stream_identifier != 1 || length != 5 => return Err(ResponseFrameRefusal),
            3 if stream_identifier != 1 || length != 4 => return Err(ResponseFrameRefusal),
            4 if stream_identifier != 0 || length % 6 != 0 || (flags & 1 != 0 && length != 0) => {
                return Err(ResponseFrameRefusal);
            }
            5 => return Err(ResponseFrameRefusal),
            6 if stream_identifier != 0 || length != 8 => return Err(ResponseFrameRefusal),
            7 if stream_identifier != 0 || length < 8 => return Err(ResponseFrameRefusal),
            8 if !matches!(stream_identifier, 0 | 1) || length != 4 => {
                return Err(ResponseFrameRefusal);
            }
            9 if stream_identifier != 1 || !self.continuation => return Err(ResponseFrameRefusal),
            _ => {}
        }
        let mut remaining = length;
        let mut padding = 0;
        if kind == 1 {
            if self.head_started {
                self.trailer_started = true;
                self.encoded_header_bytes = 0;
            }
            self.head_started = true;
            if flags & 8 != 0 {
                remaining = remaining.checked_sub(1).ok_or(ResponseFrameRefusal)?;
                padding = usize::from(stream.read_u8().await.map_err(|_| ResponseFrameRefusal)?);
            }
            if flags & 0x20 != 0 {
                remaining = remaining.checked_sub(5).ok_or(ResponseFrameRefusal)?;
                let mut priority = [0; 5];
                stream.read_exact(&mut priority).await.map_err(|_| ResponseFrameRefusal)?;
                if u32::from_be_bytes(priority[..4].try_into().unwrap()) & 0x7fff_ffff == 1 {
                    return Err(ResponseFrameRefusal);
                }
            }
            remaining = remaining.checked_sub(padding).ok_or(ResponseFrameRefusal)?;
        }
        if matches!(kind, 1 | 9) {
            self.encoded_header_bytes = match self
                .encoded_header_bytes
                .checked_add(remaining as u64)
                .filter(|bytes| *bytes <= self.maximum_header_bytes)
            {
                Some(bytes) => bytes,
                None => {
                    self.header_limit_exceeded = true;
                    return Err(ResponseFrameRefusal);
                }
            };
            self.continuation = flags & 4 == 0;
        }
        let mut payload = SensitivePayload(vec![0; remaining]);
        stream.read_exact(&mut payload.0).await.map_err(|_| ResponseFrameRefusal)?;
        let mut discarded_padding = [0; 255];
        stream
            .read_exact(&mut discarded_padding[..padding])
            .await
            .map_err(|_| ResponseFrameRefusal)?;
        validate_control_payload(kind, flags, stream_identifier, &payload.0)?;
        if matches!(kind, 0 | 1) && flags & 1 != 0 {
            self.stream_ended = true;
        }
        if kind == 3 {
            self.stream_ended = true;
            self.stream_reset = true;
        }
        Ok(Some(ResponseFrame {
            kind,
            flags,
            stream_identifier,
            payload: std::mem::take(&mut payload.0),
        }))
    }
}

/// RFC 9113 sections 6.1, 6.3, 6.5.2 and 6.9. The frame reader has already
/// checked fixed payload lengths. Unknown settings remain ignorable and
/// repeated settings remain ordered; neither is silently normalized here.
fn validate_control_payload(
    kind: u8,
    flags: u8,
    stream_identifier: u32,
    payload: &[u8],
) -> Result<(), ResponseFrameRefusal> {
    match kind {
        0 if flags & 8 != 0 => {
            let padding = usize::from(*payload.first().ok_or(ResponseFrameRefusal)?);
            if padding >= payload.len() {
                return Err(ResponseFrameRefusal);
            }
        }
        2 => {
            let dependency = u32::from_be_bytes(payload[..4].try_into().unwrap()) & 0x7fff_ffff;
            if dependency == stream_identifier {
                return Err(ResponseFrameRefusal);
            }
        }
        4 => {
            for setting in payload.chunks_exact(6) {
                let identifier = u16::from_be_bytes(setting[..2].try_into().unwrap());
                let value = u32::from_be_bytes(setting[2..].try_into().unwrap());
                if identifier == 2 && value != 0
                    || identifier == 4 && value > 0x7fff_ffff
                    || identifier == 5 && !(16_384..=16_777_215).contains(&value)
                {
                    return Err(ResponseFrameRefusal);
                }
            }
        }
        8 => {
            let increment = u32::from_be_bytes(payload.try_into().unwrap()) & 0x7fff_ffff;
            if increment == 0 {
                return Err(ResponseFrameRefusal);
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt;
    use tokio::time::{Duration, timeout};

    fn frame(kind: u8, flags: u8, stream: u32, payload: &[u8]) -> Vec<u8> {
        let size = (payload.len() as u32).to_be_bytes();
        let mut bytes = vec![size[1], size[2], size[3], kind, flags];
        bytes.extend_from_slice(&stream.to_be_bytes());
        bytes.extend_from_slice(payload);
        bytes
    }

    async fn initialized(maximum: u64) -> ResponseFrameReader {
        let mut reader = ResponseFrameReader::new();
        reader.maximum_header_bytes = maximum;
        reader.read(&mut frame(4, 0, 0, &[]).as_slice()).await.unwrap();
        reader
    }

    #[tokio::test]
    async fn settings_enforce_server_values_without_discarding_order_or_extensions() {
        for (identifier, value, valid) in [
            (2_u16, 0_u32, true),
            (2, 1, false),
            (2, 2, false),
            (4, 0, true),
            (4, 0x7fff_ffff, true),
            (4, 0x8000_0000, false),
            (5, 16_383, false),
            (5, 16_384, true),
            (5, 16_777_215, true),
            (5, 16_777_216, false),
            (0xffff, u32::MAX, true),
            (1, u32::MAX, true),
            (3, 0, true),
        ] {
            let mut setting = identifier.to_be_bytes().to_vec();
            setting.extend_from_slice(&value.to_be_bytes());
            let result =
                ResponseFrameReader::new().read(&mut frame(4, 0, 0, &setting).as_slice()).await;
            assert_eq!(result.is_ok(), valid, "setting {identifier}: {value}");
        }
        let ordered = [0, 4, 0, 0, 0, 0, 0, 4, 0, 0, 0, 1];
        let accepted = ResponseFrameReader::new()
            .read(&mut frame(4, 0, 0, &ordered).as_slice())
            .await
            .unwrap();
        assert_eq!(accepted.payload, ordered);
    }

    #[tokio::test]
    async fn window_updates_and_priority_dependencies_validate_reserved_bits() {
        for stream in [0, 1] {
            for increment in [0_u32, 0x8000_0000, 1, 0x8000_0001, 0x7fff_ffff] {
                let result = initialized(64)
                    .await
                    .read(&mut frame(8, 0, stream, &increment.to_be_bytes()).as_slice())
                    .await;
                assert_eq!(result.is_ok(), increment & 0x7fff_ffff != 0);
            }
        }
        for dependency in [0_u32, 1, 0x8000_0001, 3] {
            let mut payload = dependency.to_be_bytes().to_vec();
            payload.push(0);
            assert_eq!(
                initialized(64).await.read(&mut frame(2, 0, 1, &payload).as_slice()).await.is_ok(),
                dependency & 0x7fff_ffff != 1
            );
        }
    }

    #[tokio::test]
    async fn data_padding_is_validated_and_preserved_for_flow_control() {
        for (payload, valid) in [
            (&[][..], false),
            (&[0][..], true),
            (&[1][..], false),
            (&[1, 0][..], true),
            (&[2, 0][..], false),
            (&[1, b'x', 0][..], true),
        ] {
            let mut reader = initialized(64).await;
            reader.read(&mut frame(1, 4, 1, b"a").as_slice()).await.unwrap();
            let result = reader.read(&mut frame(0, 8, 1, payload).as_slice()).await;
            assert_eq!(result.is_ok(), valid);
            if valid {
                assert_eq!(result.unwrap().payload, payload);
            }
        }
    }

    #[tokio::test]
    async fn a_reset_prevents_later_response_headers_or_data() {
        for started in [false, true] {
            for later in [frame(1, 4, 1, b"a"), frame(0, 1, 1, b"body")] {
                let mut reader = initialized(64).await;
                if started {
                    reader.read(&mut frame(1, 4, 1, b"a").as_slice()).await.unwrap();
                }
                let reset = reader.read(&mut frame(3, 0, 1, &[0; 4]).as_slice()).await.unwrap();
                assert_eq!(reset.kind, 3);
                assert!(reader.read(&mut later.as_slice()).await.is_err());
            }
        }
    }

    #[tokio::test]
    async fn header_fragments_charge_only_encoded_bytes_and_accept_the_exact_bound() {
        let mut reader = initialized(4).await;
        // Padding and priority are not HPACK bytes. A reserved stream bit is
        // ignored per HTTP/2, not interpreted as a different stream.
        let first = frame(1, 0x28, 0x8000_0001, &[2, 0, 0, 0, 0, 15, b'a', b'b', 0, 0]);
        let decoded = reader.read(&mut first.as_slice()).await.unwrap();
        assert_eq!(decoded.payload, b"ab");
        assert_eq!(format!("{decoded:?}"), "ResponseFrame([redacted])");
        assert_eq!(reader.encoded_header_bytes, 2);
        let last = reader.read(&mut frame(9, 4, 1, b"cd").as_slice()).await.unwrap();
        assert_eq!(last.payload, b"cd");
        assert_eq!(reader.encoded_header_bytes, 4);
        reader.read(&mut frame(0, 1, 1, b"body").as_slice()).await.unwrap();
        assert!(reader.read(&mut frame(0, 0, 1, b"extra").as_slice()).await.is_err());
    }

    #[tokio::test]
    async fn oversize_frames_and_header_blocks_fail_without_reading_the_payload() {
        for continuation in [false, true] {
            let mut reader = initialized(4).await;
            if continuation {
                reader.read(&mut frame(1, 0, 1, b"1234").as_slice()).await.unwrap();
            }
            let (mut writer, mut input) = tokio::io::duplex(32);
            let bytes = frame(
                if continuation { 9 } else { 1 },
                4,
                1,
                if continuation { b"x" } else { b"12345" },
            );
            writer.write_all(&bytes[..9]).await.unwrap();
            assert!(
                timeout(Duration::from_millis(100), reader.read(&mut input))
                    .await
                    .unwrap()
                    .is_err()
            );
            assert!(reader.poisoned);
        }
        let mut reader = initialized(u64::MAX).await;
        let (mut writer, mut input) = tokio::io::duplex(32);
        writer.write_all(&[0, 64, 1, 1, 4, 0, 0, 0, 1]).await.unwrap();
        assert!(
            timeout(Duration::from_millis(100), reader.read(&mut input)).await.unwrap().is_err()
        );
    }

    #[tokio::test]
    async fn continuation_order_trailers_and_push_are_refused() {
        for bytes in [
            frame(0, 0, 1, b"x"),
            frame(6, 0, 0, &[0; 8]),
            frame(9, 4, 3, b"x"),
            frame(1, 4, 1, b"x"),
        ] {
            let mut reader = initialized(64).await;
            reader.read(&mut frame(1, 0, 1, b"a").as_slice()).await.unwrap();
            assert!(reader.read(&mut bytes.as_slice()).await.is_err());
        }
        for bytes in [
            frame(1, 5, 1, &[]),
            frame(1, 5, 1, b"trailer"),
            frame(9, 4, 1, b"x"),
            frame(5, 4, 1, &[0; 4]),
        ] {
            let mut reader = initialized(64).await;
            reader.read(&mut frame(1, 4, 1, b"a").as_slice()).await.unwrap();
            assert!(reader.read(&mut bytes.as_slice()).await.is_err());
        }
    }

    #[tokio::test]
    async fn malformed_preambles_streams_padding_and_priority_are_refused() {
        for bytes in
            [frame(4, 1, 0, &[]), frame(4, 0, 1, &[]), frame(4, 0, 0, &[0]), frame(1, 4, 1, b"a")]
        {
            assert!(ResponseFrameReader::new().read(&mut bytes.as_slice()).await.is_err());
        }
        for bytes in [
            frame(1, 4, 3, b"a"),
            frame(1, 12, 1, &[]),
            frame(1, 12, 1, &[2, 0]),
            frame(1, 36, 1, &[0; 4]),
            frame(1, 36, 1, &[0, 0, 0, 1, 0]),
        ] {
            assert!(initialized(64).await.read(&mut bytes.as_slice()).await.is_err());
        }
    }

    #[tokio::test]
    async fn peer_settings_do_not_enlarge_the_receiving_frame_limit() {
        let mut reader = ResponseFrameReader::new();
        // The peer's maximum frame size constrains our sends, not its sends.
        reader.read(&mut frame(4, 0, 0, &[0, 5, 0, 255, 255, 255]).as_slice()).await.unwrap();
        let maximum = frame(1, 4, 1, &vec![0; MAXIMUM_FRAME_BYTES]);
        assert_eq!(
            reader.read(&mut maximum.as_slice()).await.unwrap().payload.len(),
            MAXIMUM_FRAME_BYTES
        );
        let oversized = frame(0, 0, 1, &vec![0; MAXIMUM_FRAME_BYTES + 1]);
        assert!(reader.read(&mut oversized.as_slice()).await.is_err());
    }

    #[tokio::test]
    async fn end_stream_on_headers_still_requires_all_continuations() {
        let mut reader = initialized(2).await;
        reader.read(&mut frame(1, 1, 1, b"a").as_slice()).await.unwrap();
        assert!(reader.stream_ended);
        reader.read(&mut frame(9, 4, 1, b"b").as_slice()).await.unwrap();
        assert!(!reader.continuation);
        assert!(reader.read(&mut frame(0, 1, 1, &[]).as_slice()).await.is_err());
    }

    #[tokio::test]
    async fn clean_end_requires_complete_headers_and_end_stream_and_is_single_use() {
        assert!(ResponseFrameReader::new().read_next(&mut &[][..]).await.is_err());
        for head in [None, Some((4, b"a".as_slice())), Some((1, b"a".as_slice()))] {
            let mut reader = initialized(64).await;
            if let Some((flags, payload)) = head {
                reader.read(&mut frame(1, flags, 1, payload).as_slice()).await.unwrap();
            }
            assert!(reader.read_next(&mut &[][..]).await.is_err());
        }
        let mut reader = initialized(64).await;
        reader.read(&mut frame(1, 1, 1, b"a").as_slice()).await.unwrap();
        reader.read(&mut frame(9, 4, 1, b"b").as_slice()).await.unwrap();
        // Connection-level control frames remain valid after END_STREAM.
        reader.read(&mut frame(6, 0, 0, &[0; 8]).as_slice()).await.unwrap();
        assert!(matches!(reader.read_next(&mut &[][..]).await.unwrap(), FrameRead::End(_)));
        assert!(reader.read_next(&mut &[][..]).await.is_err());
    }

    #[tokio::test]
    async fn truncated_or_trailing_frames_never_produce_clean_end() {
        let ping = frame(6, 0, 0, &[0; 8]);
        let mut invalid: Vec<Vec<u8>> = (1..ping.len()).map(|n| ping[..n].to_vec()).collect();
        invalid.extend([frame(1, 5, 1, &[]), frame(0, 1, 1, &[])]);
        for bytes in invalid {
            let mut reader = initialized(64).await;
            reader.read(&mut frame(1, 5, 1, b"a").as_slice()).await.unwrap();
            assert!(reader.read_next(&mut bytes.as_slice()).await.is_err());
            assert!(reader.read_next(&mut &[][..]).await.is_err());
        }
    }

    #[tokio::test]
    async fn cancelled_end_wait_cannot_later_issue_proof() {
        let mut reader = initialized(64).await;
        reader.read(&mut frame(1, 5, 1, b"a").as_slice()).await.unwrap();
        let (writer, mut input) = tokio::io::duplex(32);
        assert!(timeout(Duration::from_millis(10), reader.read_next(&mut input)).await.is_err());
        drop(writer);
        assert!(reader.read_next(&mut input).await.is_err());
    }

    #[tokio::test]
    async fn reset_never_produces_completion_proof() {
        for flags in [4, 5] {
            let mut reader = initialized(64).await;
            reader.read(&mut frame(1, flags, 1, b"a").as_slice()).await.unwrap();
            reader.read(&mut frame(3, 0, 1, &[0; 4]).as_slice()).await.unwrap();
            assert!(reader.read_next(&mut &[][..]).await.is_err());
        }
    }

    #[tokio::test]
    async fn cancelled_or_truncated_frame_reads_cannot_resume_as_new_frames() {
        let mut reader = initialized(64).await;
        let (mut writer, mut input) = tokio::io::duplex(32);
        let bytes = frame(1, 4, 1, b"ab");
        writer.write_all(&bytes[..10]).await.unwrap();
        assert!(timeout(Duration::from_millis(10), reader.read(&mut input)).await.is_err());
        assert!(reader.poisoned);
        assert!(
            timeout(Duration::from_millis(100), reader.read(&mut input)).await.unwrap().is_err()
        );
        let mut reader = initialized(64).await;
        assert!(reader.read(&mut bytes[..10].as_ref()).await.is_err());
        assert!(reader.read(&mut frame(1, 4, 1, b"ab").as_slice()).await.is_err());
    }
}
