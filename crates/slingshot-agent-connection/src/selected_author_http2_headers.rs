//! Incremental decoded HTTP/2 response fields for an HPACK decoder to feed.
//!
//! The decoder must feed bytes here before collecting a decoded field itself.
//! This layer does not decode HPACK or authenticate a command response. Its
//! finished head still needs the shared route/media/framing policy and a proven
//! complete body before any operation outcome can be exposed.

use crate::author_hypertext_transfer_protocol_policy::HeadBounds;
use http::{HeaderMap, HeaderName, HeaderValue, StatusCode};

/// No peer-controlled field content is included in a refusal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the author HTTP/2 decoded response head is invalid")]
pub struct DecodedHeadRefusal;

struct Field {
    name: Vec<u8>,
    value: Vec<u8>,
    in_value: bool,
}

/// A bounded decoded head, not evidence of a complete HTTP exchange.
pub struct DecodedResponseHead {
    status: StatusCode,
    headers: HeaderMap,
}

impl core::fmt::Debug for DecodedResponseHead {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("DecodedResponseHead([redacted])")
    }
}

impl DecodedResponseHead {
    /// Passes the bounded head to the shared response gate after body framing
    /// has been proven. These parts alone cannot establish a route outcome.
    pub fn into_parts(self) -> (StatusCode, HeaderMap) {
        (self.status, self.headers)
    }
}

/// Counts each decoded field, including :status. The decoded aggregate charges
/// name and value bytes plus four delimiter bytes per field and the final two
/// terminator bytes. The encoded block has its separate wire-reader budget.
pub struct DecodedHeadReader {
    bounds: HeadBounds,
    fields: u64,
    bytes: u64,
    status: Option<StatusCode>,
    headers: HeaderMap,
    current: Option<Field>,
    poisoned: bool,
}

impl core::fmt::Debug for DecodedHeadReader {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("DecodedHeadReader([redacted])")
    }
}

impl Default for DecodedHeadReader {
    fn default() -> Self {
        Self::new()
    }
}

impl DecodedHeadReader {
    /// Uses only the installed transport contract's decoded limits.
    pub fn new() -> Self {
        Self::with_bounds(HeadBounds::embedded())
    }

    fn with_bounds(bounds: HeadBounds) -> Self {
        Self {
            bounds,
            fields: 0,
            bytes: 0,
            status: None,
            headers: HeaderMap::new(),
            current: None,
            poisoned: false,
        }
    }

    fn advance(
        &mut self,
        action: impl FnOnce(&mut Self) -> Result<(), DecodedHeadRefusal>,
    ) -> Result<(), DecodedHeadRefusal> {
        if self.poisoned {
            return Err(DecodedHeadRefusal);
        }
        self.poisoned = true;
        action(self)?;
        self.poisoned = false;
        Ok(())
    }

    fn charge(&mut self, bytes: u64) -> Result<(), DecodedHeadRefusal> {
        self.bytes = self
            .bytes
            .checked_add(bytes)
            .filter(|n| *n <= self.bounds.head_bytes)
            .ok_or(DecodedHeadRefusal)?;
        Ok(())
    }

    /// Starts one decoded field before any name/value storage is allocated.
    pub fn begin_field(&mut self) -> Result<(), DecodedHeadRefusal> {
        self.advance(|reader| {
            if reader.current.is_some() || reader.fields >= reader.bounds.field_count {
                return Err(DecodedHeadRefusal);
            }
            reader.charge(4)?;
            reader.fields += 1;
            reader.current = Some(Field { name: Vec::new(), value: Vec::new(), in_value: false });
            Ok(())
        })
    }

    /// Feeds a decoded lowercase field-name octet, not compressed HPACK bytes.
    pub fn name_byte(&mut self, byte: u8) -> Result<(), DecodedHeadRefusal> {
        self.advance(|reader| {
            let field = reader.current.as_ref().ok_or(DecodedHeadRefusal)?;
            if field.in_value
                || !(byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || b"!#$%&'*+-.^_`|~".contains(&byte)
                    || byte == b':' && field.name.is_empty())
            {
                return Err(DecodedHeadRefusal);
            }
            if field.name.len() as u64 >= reader.bounds.field_bytes {
                return Err(DecodedHeadRefusal);
            }
            reader.charge(1)?;
            reader.current.as_mut().unwrap().name.push(byte);
            Ok(())
        })
    }

    /// Ends the name and begins its value; response pseudo-fields are closed.
    pub fn begin_value(&mut self) -> Result<(), DecodedHeadRefusal> {
        self.advance(|reader| {
            let field = reader.current.as_mut().ok_or(DecodedHeadRefusal)?;
            if field.in_value || field.name.is_empty() {
                return Err(DecodedHeadRefusal);
            }
            if field.name.starts_with(b":") {
                if field.name != b":status" || reader.status.is_some() || reader.fields != 1 {
                    return Err(DecodedHeadRefusal);
                }
            } else if reader.status.is_none() {
                return Err(DecodedHeadRefusal);
            }
            field.in_value = true;
            Ok(())
        })
    }

    /// Feeds one value octet after checking the field and aggregate bounds.
    pub fn value_byte(&mut self, byte: u8) -> Result<(), DecodedHeadRefusal> {
        self.advance(|reader| {
            let field = reader.current.as_ref().ok_or(DecodedHeadRefusal)?;
            if !field.in_value
                || matches!(byte, 0 | b'\r' | b'\n')
                || field.value.is_empty() && matches!(byte, b' ' | b'\t')
            {
                return Err(DecodedHeadRefusal);
            }
            let size = field.name.len().checked_add(field.value.len()).ok_or(DecodedHeadRefusal)?;
            if size as u64 >= reader.bounds.field_bytes {
                return Err(DecodedHeadRefusal);
            }
            reader.charge(1)?;
            reader.current.as_mut().unwrap().value.push(byte);
            Ok(())
        })
    }

    /// Commits only a complete, semantically valid bounded field.
    pub fn end_field(&mut self) -> Result<(), DecodedHeadRefusal> {
        self.advance(|reader| {
            let field = reader.current.take().ok_or(DecodedHeadRefusal)?;
            if !field.in_value || field.value.last().is_some_and(|b| matches!(b, b' ' | b'\t')) {
                return Err(DecodedHeadRefusal);
            }
            if field.name == b":status" {
                let status =
                    StatusCode::from_bytes(&field.value).map_err(|_| DecodedHeadRefusal)?;
                if status.as_u16() < 200 || status.as_u16() >= 600 || status.is_redirection() {
                    return Err(DecodedHeadRefusal);
                }
                reader.status = Some(status);
            } else {
                if [
                    b"connection".as_slice(),
                    b"proxy-connection",
                    b"keep-alive",
                    b"transfer-encoding",
                    b"te",
                    b"upgrade",
                    b"trailer",
                    b"alt-svc",
                ]
                .contains(&field.name.as_slice())
                {
                    return Err(DecodedHeadRefusal);
                }
                let name = HeaderName::from_bytes(&field.name).map_err(|_| DecodedHeadRefusal)?;
                let value =
                    HeaderValue::from_bytes(&field.value).map_err(|_| DecodedHeadRefusal)?;
                reader.headers.append(name, value);
            }
            Ok(())
        })
    }

    /// Completes only a whole head. Partial fields and any earlier refusal fail
    /// closed; no caller can retrieve the partially collected header map.
    pub fn finish(mut self) -> Result<DecodedResponseHead, DecodedHeadRefusal> {
        if self.poisoned || self.current.is_some() {
            return Err(DecodedHeadRefusal);
        }
        self.charge(2)?;
        Ok(DecodedResponseHead {
            status: self.status.ok_or(DecodedHeadRefusal)?,
            headers: self.headers,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(
        reader: &mut DecodedHeadReader,
        name: &[u8],
        value: &[u8],
    ) -> Result<(), DecodedHeadRefusal> {
        reader.begin_field()?;
        for byte in name {
            reader.name_byte(*byte)?;
        }
        reader.begin_value()?;
        for byte in value {
            reader.value_byte(*byte)?;
        }
        reader.end_field()
    }

    #[test]
    fn exact_decoded_bounds_include_status_fields_and_separators() {
        let mut reader = DecodedHeadReader::with_bounds(HeadBounds {
            field_bytes: 10,
            field_count: 2,
            head_bytes: 22,
        });
        field(&mut reader, b":status", b"200").unwrap();
        field(&mut reader, b"x", b"y").unwrap();
        let head = reader.finish().unwrap();
        assert_eq!(format!("{head:?}"), "DecodedResponseHead([redacted])");
        let (status, headers) = head.into_parts();
        assert_eq!(status, 200);
        assert_eq!(headers["x"], "y");
        let mut reader = DecodedHeadReader::with_bounds(HeadBounds {
            field_bytes: 10,
            field_count: 2,
            head_bytes: 21,
        });
        field(&mut reader, b":status", b"200").unwrap();
        field(&mut reader, b"x", b"y").unwrap();
        assert!(reader.finish().is_err());
    }

    #[test]
    fn excess_bytes_and_fields_are_not_collected_and_poison_the_reader() {
        let mut reader = DecodedHeadReader::with_bounds(HeadBounds {
            field_bytes: 10,
            field_count: 2,
            head_bytes: 64,
        });
        field(&mut reader, b":status", b"200").unwrap();
        reader.begin_field().unwrap();
        reader.name_byte(b'x').unwrap();
        reader.begin_value().unwrap();
        for _ in 0..9 {
            reader.value_byte(b'y').unwrap();
        }
        assert!(reader.value_byte(b'z').is_err());
        assert_eq!(reader.current.as_ref().unwrap().value, b"yyyyyyyyy");
        assert!(reader.end_field().is_err());
        assert!(reader.finish().is_err());
        let mut reader = DecodedHeadReader::with_bounds(HeadBounds {
            field_bytes: 10,
            field_count: 1,
            head_bytes: 64,
        });
        field(&mut reader, b":status", b"200").unwrap();
        assert!(reader.begin_field().is_err());
        assert!(reader.current.is_none());
        let mut reader = DecodedHeadReader::with_bounds(HeadBounds {
            field_bytes: 10,
            field_count: 2,
            head_bytes: 19,
        });
        field(&mut reader, b":status", b"200").unwrap();
        reader.begin_field().unwrap();
        reader.name_byte(b'x').unwrap();
        reader.begin_value().unwrap();
        assert!(reader.value_byte(b'y').is_err());
        assert!(reader.current.as_ref().unwrap().value.is_empty());
    }

    #[test]
    fn status_is_required_unique_first_and_noninformational() {
        for value in [b"100".as_slice(), b"101", b"103", b"302", b"600", b"20", b"2000", b"abc"] {
            assert!(field(&mut DecodedHeadReader::new(), b":status", value).is_err());
        }
        for name in [b"x".as_slice(), b":path", b":method"] {
            assert!(field(&mut DecodedHeadReader::new(), name, b"x").is_err());
        }
        let mut reader = DecodedHeadReader::new();
        field(&mut reader, b":status", b"200").unwrap();
        assert!(field(&mut reader, b":status", b"200").is_err());
        assert!(DecodedHeadReader::new().finish().is_err());
    }

    #[test]
    fn hostile_names_values_and_transport_fields_are_refused_without_rewriting() {
        for (name, value) in [
            (b"X".as_slice(), b"y".as_slice()),
            (b"x", b" y"),
            (b"x", b"y "),
            (b"x", b"y\r"),
            (b"x", b"y\n"),
            (b"x", b"y\0"),
            (b"connection", b"close"),
            (b"proxy-connection", b"close"),
            (b"keep-alive", b"x"),
            (b"transfer-encoding", b"chunked"),
            (b"te", b"trailers"),
            (b"upgrade", b"h2c"),
            (b"trailer", b"x"),
            (b"alt-svc", b"x"),
        ] {
            let mut reader = DecodedHeadReader::new();
            field(&mut reader, b":status", b"200").unwrap();
            assert!(field(&mut reader, name, value).is_err(), "{name:?}");
            assert!(reader.finish().is_err());
        }
    }

    #[test]
    fn partial_fields_and_invalid_decoder_call_order_never_publish_a_head() {
        for stage in 0..4 {
            let mut reader = DecodedHeadReader::new();
            field(&mut reader, b":status", b"200").unwrap();
            reader.begin_field().unwrap();
            if stage >= 1 {
                reader.name_byte(b'x').unwrap();
            }
            if stage >= 2 {
                reader.begin_value().unwrap();
            }
            if stage >= 3 {
                reader.value_byte(b'y').unwrap();
            }
            assert!(reader.finish().is_err());
        }
        let mut reader = DecodedHeadReader::new();
        assert!(reader.value_byte(b'x').is_err());
        assert!(reader.begin_field().is_err());
        let mut reader = DecodedHeadReader::new();
        reader.begin_field().unwrap();
        assert!(reader.begin_field().is_err());
        let mut reader = DecodedHeadReader::with_bounds(HeadBounds {
            field_bytes: 10,
            field_count: 2,
            head_bytes: 64,
        });
        field(&mut reader, b":status", b"200").unwrap();
        reader.begin_field().unwrap();
        for _ in 0..10 {
            reader.name_byte(b'x').unwrap();
        }
        assert!(reader.name_byte(b'y').is_err());
        assert_eq!(reader.current.as_ref().unwrap().name.len(), 10);
    }

    #[test]
    fn duplicate_fields_remain_visible_to_the_shared_singleton_gate() {
        let mut reader = DecodedHeadReader::new();
        field(&mut reader, b":status", b"200").unwrap();
        field(&mut reader, b"content-type", b"application/json").unwrap();
        field(&mut reader, b"content-type", b"text/plain").unwrap();
        assert_eq!(format!("{reader:?}"), "DecodedHeadReader([redacted])");
        let (status, headers) = reader.finish().unwrap().into_parts();
        assert_eq!(headers.get_all("content-type").iter().count(), 2);
        let mut response = http::Response::new(b"{}".to_vec());
        *response.status_mut() = status;
        *response.version_mut() = http::Version::HTTP_2;
        *response.headers_mut() = headers;
        assert!(
            crate::selected_author_exchange::validate_collected_finite_response(
                crate::selected_author_exchange::CollectedFiniteResponse {
                    response,
                    framing_ambiguous: false,
                    trailer_section_present: false,
                    trailing_bytes: false,
                }
            )
            .is_err()
        );
    }
}
