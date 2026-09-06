//! One incremental HPACK response block for a fresh, single-request connection.
//! All representation forms feed the same decoded-header policy before storage.
//! Section policy belongs to the destination. A bounded next section may retain
//! compression state so a route can account for trailers before refusing them.

use crate::author_hypertext_transfer_protocol_policy::HeadBounds;
use crate::selected_author_hpack_integer::PrefixedInteger;
use crate::selected_author_hpack_string::{LiteralString, StringRefusal};
use crate::selected_author_hpack_table::{HeaderTable, MAXIMUM_DYNAMIC_TABLE_BYTES};
use crate::selected_author_http2_headers::DecodedHeadRefusal;
use crate::selected_author_http2_headers::{DecodedHeadReader, DecodedResponseHead};

/// Incremental destination for decoded HPACK fields, before capture/table storage.
/// A route supplies its own accounting and status policy without replacing HPACK.
pub trait DecodedHeaderSink {
    /// One completely decoded section, never a partial response.
    type Output;
    /// Begins one field before decoding its name.
    fn begin_field(&mut self) -> Result<(), DecodedHeadRefusal>;
    /// Checks and accepts one name octet before storage.
    fn name_byte(&mut self, byte: u8) -> Result<(), DecodedHeadRefusal>;
    /// Ends the name and starts the value.
    fn begin_value(&mut self) -> Result<(), DecodedHeadRefusal>;
    /// Checks and accepts one value octet before storage.
    fn value_byte(&mut self, byte: u8) -> Result<(), DecodedHeadRefusal>;
    /// Accepts a complete field.
    fn end_field(&mut self) -> Result<(), DecodedHeadRefusal>;
    /// Accepts a complete section.
    fn finish(self) -> Result<Self::Output, DecodedHeadRefusal>;
}
impl DecodedHeaderSink for DecodedHeadReader {
    type Output = DecodedResponseHead;
    fn begin_field(&mut self) -> Result<(), DecodedHeadRefusal> {
        self.begin_field()
    }
    fn name_byte(&mut self, byte: u8) -> Result<(), DecodedHeadRefusal> {
        self.name_byte(byte)
    }
    fn begin_value(&mut self) -> Result<(), DecodedHeadRefusal> {
        self.begin_value()
    }
    fn value_byte(&mut self, byte: u8) -> Result<(), DecodedHeadRefusal> {
        self.value_byte(byte)
    }
    fn end_field(&mut self) -> Result<(), DecodedHeadRefusal> {
        self.end_field()
    }
    fn finish(self) -> Result<Self::Output, DecodedHeadRefusal> {
        self.finish()
    }
}

/// A malformed, oversized or policy-refused block, with no private wire text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the author HPACK response block is invalid")]
pub struct BlockRefusal;

#[derive(Clone, Copy)]
enum Purpose {
    Indexed,
    Name { indexing: bool },
    Resize,
}

enum State {
    Instruction,
    Integer(PrefixedInteger, Purpose),
    NameStart { indexing: bool },
    Name(LiteralString, bool),
    ValueStart { indexing: bool },
    Value(LiteralString, bool),
    Poisoned,
}

struct Capture {
    name: Vec<u8>,
    value: Vec<u8>,
}

/// A fresh response's HPACK and decoded-header state. Any failure permanently
/// poisons it; no partial head or table is exposed.
pub struct ResponseBlock<Reader = DecodedHeadReader> {
    state: State,
    table: HeaderTable,
    headers: Reader,
    capture: Option<Capture>,
    seen_field: bool,
    encoded_bytes: u64,
    maximum_encoded_bytes: u64,
    encoded_limit_exceeded: bool,
}

impl<Reader> core::fmt::Debug for ResponseBlock<Reader> {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("ResponseBlock([redacted])")
    }
}

impl Default for ResponseBlock {
    fn default() -> Self {
        Self::new()
    }
}

impl ResponseBlock {
    /// Starts with the HTTP/2 default decoder table and installed head limits.
    pub fn new() -> Self {
        Self::with_reader(DecodedHeadReader::new(), HeadBounds::embedded().head_bytes)
    }
}

impl<Reader: DecodedHeaderSink> ResponseBlock<Reader> {
    /// Installs a route-specific decoded gate and independent compressed bound.
    pub fn with_reader(headers: Reader, maximum_encoded_bytes: u64) -> Self {
        Self {
            state: State::Instruction,
            table: HeaderTable::new(),
            headers,
            capture: None,
            seen_field: false,
            encoded_bytes: 0,
            maximum_encoded_bytes,
            encoded_limit_exceeded: false,
        }
    }

    /// The route's gate, for retrieving its redacted refusal classification.
    pub fn reader(&self) -> &Reader {
        &self.headers
    }

    /// Distinguishes a compressed-bound refusal from malformed HPACK syntax.
    pub fn encoded_limit_exceeded(&self) -> bool {
        self.encoded_limit_exceeded
    }

    /// Feeds an arbitrary fragment, including a fragment ending inside an
    /// integer, literal length, Huffman code, or dynamic-table instruction.
    pub fn push(&mut self, bytes: &[u8]) -> Result<(), BlockRefusal> {
        if matches!(self.state, State::Poisoned) {
            return Err(BlockRefusal);
        }
        for byte in bytes {
            let state = core::mem::replace(&mut self.state, State::Poisoned);
            self.encoded_bytes = match self
                .encoded_bytes
                .checked_add(1)
                .filter(|n| *n <= self.maximum_encoded_bytes)
            {
                Some(bytes) => bytes,
                None => {
                    self.encoded_limit_exceeded = true;
                    return Err(BlockRefusal);
                }
            };
            self.state = self.step(state, *byte)?;
        }
        Ok(())
    }

    fn step(&mut self, state: State, byte: u8) -> Result<State, BlockRefusal> {
        match state {
            State::Instruction => {
                let (prefix, purpose, maximum) = if byte & 0x80 != 0 {
                    (7, Purpose::Indexed, 61 + MAXIMUM_DYNAMIC_TABLE_BYTES / 32)
                } else if byte & 0xe0 == 0x20 {
                    if self.seen_field {
                        return Err(BlockRefusal);
                    }
                    (5, Purpose::Resize, MAXIMUM_DYNAMIC_TABLE_BYTES)
                } else {
                    let indexing = byte & 0x40 != 0;
                    (
                        if indexing { 6 } else { 4 },
                        Purpose::Name { indexing },
                        61 + MAXIMUM_DYNAMIC_TABLE_BYTES / 32,
                    )
                };
                if !matches!(purpose, Purpose::Resize) {
                    self.seen_field = true;
                }
                let integer =
                    PrefixedInteger::start(byte, prefix, maximum).map_err(|_| BlockRefusal)?;
                if let Some(value) = integer.value() {
                    self.integer(value, purpose)
                } else {
                    Ok(State::Integer(integer, purpose))
                }
            }
            State::Integer(mut integer, purpose) => {
                match integer.push(byte).map_err(|_| BlockRefusal)? {
                    Some(value) => self.integer(value, purpose),
                    None => Ok(State::Integer(integer, purpose)),
                }
            }
            State::NameStart { indexing } => {
                let string =
                    LiteralString::start(byte, self.maximum_encoded_bytes - self.encoded_bytes)
                        .map_err(|_| {
                            self.encoded_limit_exceeded = true;
                            BlockRefusal
                        })?;
                if string.is_complete() {
                    return Err(BlockRefusal);
                }
                Ok(State::Name(string, indexing))
            }
            State::ValueStart { indexing } => {
                let string =
                    LiteralString::start(byte, self.maximum_encoded_bytes - self.encoded_bytes)
                        .map_err(|_| {
                            self.encoded_limit_exceeded = true;
                            BlockRefusal
                        })?;
                if string.is_complete() {
                    string.finish().map_err(|_| BlockRefusal)?;
                    self.end_field(indexing)
                } else {
                    Ok(State::Value(string, indexing))
                }
            }
            State::Name(mut string, indexing) => {
                string
                    .push(byte, |symbol| emit(&mut self.headers, &mut self.capture, symbol, true))
                    .map_err(|_| {
                        self.encoded_limit_exceeded |= string.encoded_limit_exceeded();
                        BlockRefusal
                    })?;
                self.require_remaining_literal_budget(&string)?;
                if string.is_complete() {
                    string.finish().map_err(|_| BlockRefusal)?;
                    self.headers.begin_value().map_err(|_| BlockRefusal)?;
                    Ok(State::ValueStart { indexing })
                } else {
                    Ok(State::Name(string, indexing))
                }
            }
            State::Value(mut string, indexing) => {
                string
                    .push(byte, |symbol| emit(&mut self.headers, &mut self.capture, symbol, false))
                    .map_err(|_| {
                        self.encoded_limit_exceeded |= string.encoded_limit_exceeded();
                        BlockRefusal
                    })?;
                self.require_remaining_literal_budget(&string)?;
                if string.is_complete() {
                    string.finish().map_err(|_| BlockRefusal)?;
                    self.end_field(indexing)
                } else {
                    Ok(State::Value(string, indexing))
                }
            }
            State::Poisoned => Err(BlockRefusal),
        }
    }

    fn require_remaining_literal_budget(
        &mut self,
        string: &LiteralString,
    ) -> Result<(), BlockRefusal> {
        if string
            .remaining_encoded_bytes()
            .is_some_and(|remaining| remaining > self.maximum_encoded_bytes - self.encoded_bytes)
        {
            self.encoded_limit_exceeded = true;
            return Err(BlockRefusal);
        }
        Ok(())
    }

    fn integer(&mut self, value: u64, purpose: Purpose) -> Result<State, BlockRefusal> {
        match purpose {
            Purpose::Resize => {
                self.table.resize(value).map_err(|_| BlockRefusal)?;
                Ok(State::Instruction)
            }
            Purpose::Indexed => {
                let field = self.table.lookup(value).map_err(|_| BlockRefusal)?;
                self.headers.begin_field().map_err(|_| BlockRefusal)?;
                for byte in field.name() {
                    self.headers.name_byte(*byte).map_err(|_| BlockRefusal)?;
                }
                self.headers.begin_value().map_err(|_| BlockRefusal)?;
                for byte in field.value() {
                    self.headers.value_byte(*byte).map_err(|_| BlockRefusal)?;
                }
                self.headers.end_field().map_err(|_| BlockRefusal)?;
                Ok(State::Instruction)
            }
            Purpose::Name { indexing } => {
                self.headers.begin_field().map_err(|_| BlockRefusal)?;
                self.capture = indexing.then(|| Capture { name: Vec::new(), value: Vec::new() });
                if value == 0 {
                    return Ok(State::NameStart { indexing });
                }
                let field = self.table.lookup(value).map_err(|_| BlockRefusal)?;
                for byte in field.name() {
                    emit(&mut self.headers, &mut self.capture, *byte, true)
                        .map_err(|_| BlockRefusal)?;
                }
                self.headers.begin_value().map_err(|_| BlockRefusal)?;
                Ok(State::ValueStart { indexing })
            }
        }
    }

    fn end_field(&mut self, indexing: bool) -> Result<State, BlockRefusal> {
        self.headers.end_field().map_err(|_| BlockRefusal)?;
        if indexing {
            if let Some(capture) = self.capture.take() {
                self.table.insert(&capture.name, &capture.value).map_err(|_| BlockRefusal)?;
            } else {
                // A valid field larger than the fixed decoder-table allowance
                // evicts all entries; it is not itself a compression error.
                self.table.clear();
            }
        }
        Ok(State::Instruction)
    }

    /// Called only at a wire-proven END_HEADERS. No body/framing evidence is
    /// implied, and incomplete or previously refused blocks cannot finish.
    pub fn finish(self) -> Result<Reader::Output, BlockRefusal> {
        if !matches!(self.state, State::Instruction) {
            return Err(BlockRefusal);
        }
        self.headers.finish().map_err(|_| BlockRefusal)
    }

    /// Finishes this section and retains the same connection's compression table
    /// for the next bounded section. This grants no permission to accept trailers
    /// or another response: the route must still enforce those checkpoints.
    pub fn finish_with_next_reader<Next: DecodedHeaderSink>(
        self,
        reader: Next,
        maximum_encoded_bytes: u64,
    ) -> Result<(Reader::Output, ResponseBlock<Next>), BlockRefusal> {
        if !matches!(self.state, State::Instruction) {
            return Err(BlockRefusal);
        }
        let output = self.headers.finish().map_err(|_| BlockRefusal)?;
        let mut next = ResponseBlock::with_reader(reader, maximum_encoded_bytes);
        next.table = self.table;
        Ok((output, next))
    }
}

fn emit<Reader: DecodedHeaderSink>(
    headers: &mut Reader,
    capture: &mut Option<Capture>,
    byte: u8,
    name: bool,
) -> Result<(), StringRefusal> {
    if name { headers.name_byte(byte) } else { headers.value_byte(byte) }
        .map_err(|_| StringRefusal)?;
    if let Some(field) = capture {
        if field.name.len() + field.value.len() + 32 >= MAXIMUM_DYNAMIC_TABLE_BYTES as usize {
            *capture = None;
        } else if name {
            field.name.push(byte);
        } else {
            field.value.push(byte);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexed_repetition_cannot_bypass_the_decoded_field_count() {
        let maximum = HeadBounds::embedded().field_count as usize;
        for extra in [0, 1] {
            let mut wire = vec![0x88];
            new_field(&mut wire, 0x40, b"x", b"a");
            wire.extend(core::iter::repeat_n(0xbe, maximum - 2 + extra));
            let mut block = ResponseBlock::new();
            assert_eq!(block.push(&wire).is_ok(), extra == 0);
            assert_eq!(block.finish().is_ok(), extra == 0);
        }
    }

    #[tokio::test]
    async fn wire_header_continuations_feed_the_same_block_decoder() {
        use crate::selected_author_http2_frames::ResponseFrameReader;
        fn frame(kind: u8, flags: u8, payload: &[u8]) -> Vec<u8> {
            let length = (payload.len() as u32).to_be_bytes();
            let mut bytes = vec![length[1], length[2], length[3], kind, flags, 0, 0, 0, 1];
            bytes.extend_from_slice(payload);
            bytes
        }
        let mut frames = ResponseFrameReader::new();
        frames.read(&mut [0, 0, 0, 4, 0, 0, 0, 0, 0].as_slice()).await.unwrap();
        let mut wire = vec![0x88, 0x5f];
        literal(&mut wire, b"application/json");
        let first = frames.read(&mut frame(1, 1, &wire[..5]).as_slice()).await.unwrap();
        let last = frames.read(&mut frame(9, 4, &wire[5..]).as_slice()).await.unwrap();
        let mut block = ResponseBlock::new();
        block.push(&first.payload).unwrap();
        block.push(&last.payload).unwrap();
        assert_eq!(last.flags & 4, 4);
        let (status, fields) = block.finish().unwrap().into_parts();
        assert_eq!(status, 200);
        assert_eq!(fields["content-type"], "application/json");
    }

    fn literal(wire: &mut Vec<u8>, bytes: &[u8]) {
        let length = bytes.len();
        if length < 127 {
            wire.push(length as u8);
        } else {
            wire.push(127);
            let mut remaining = length - 127;
            while remaining >= 128 {
                wire.push((remaining as u8 & 127) | 128);
                remaining >>= 7;
            }
            wire.push(remaining as u8);
        }
        wire.extend_from_slice(bytes);
    }

    fn new_field(wire: &mut Vec<u8>, representation: u8, name: &[u8], value: &[u8]) {
        wire.push(representation);
        literal(wire, name);
        literal(wire, value);
    }

    #[test]
    fn every_representation_and_fragment_split_reaches_the_same_bounded_head() {
        let mut wire = vec![0x88];
        new_field(&mut wire, 0x40, b"x", b"a");
        wire.push(0xbe); // Dynamic indexed field 62.
        wire.extend_from_slice(&[0x0f, 47]); // Nonindexed literal, dynamic name 62.
        literal(&mut wire, b"b");
        new_field(&mut wire, 0x10, b"z", b""); // Never indexed.
        wire.push(0x5f);
        literal(&mut wire, b"application/json"); // Static name 31.
        for split in 0..=wire.len() {
            let mut block = ResponseBlock::new();
            block.push(&wire[..split]).unwrap();
            block.push(&wire[split..]).unwrap();
            let (status, headers) = block.finish().unwrap().into_parts();
            assert_eq!(status, 200);
            assert_eq!(
                headers.get_all("x").iter().map(|value| value.as_bytes()).collect::<Vec<_>>(),
                vec![b"a".as_slice(), b"a", b"b"]
            );
            assert_eq!(headers["content-type"], "application/json");
            assert_eq!(headers["z"], "");
        }
        let mut block = ResponseBlock::new();
        for byte in &wire {
            block.push(&[*byte]).unwrap();
        }
        block.finish().unwrap();
    }

    #[test]
    fn huffman_literals_parse_without_collecting_compressed_strings() {
        let wire = [
            0x88, 0x40, 0x88, 0x25, 0xa8, 0x49, 0xe9, 0x5b, 0xa9, 0x7d, 0x7f, 0x89, 0x25, 0xa8,
            0x49, 0xe9, 0x5b, 0xb8, 0xe8, 0xb4, 0xbf,
        ];
        for split in 0..=wire.len() {
            let mut block = ResponseBlock::new();
            block.push(&wire[..split]).unwrap();
            block.push(&wire[split..]).unwrap();
            let (_, fields) = block.finish().unwrap().into_parts();
            assert_eq!(fields["custom-key"], "custom-value");
        }
    }

    #[test]
    fn resize_eviction_and_nonindexing_do_not_create_phantom_references() {
        let mut wire = vec![0x3f, 3, 0x88]; // Capacity 34.
        new_field(&mut wire, 0x40, b"x", b"a");
        new_field(&mut wire, 0x40, b"y", b"b");
        for (index, valid) in [(0xbe, true), (0xbf, false)] {
            let mut block = ResponseBlock::new();
            block.push(&wire).unwrap();
            assert_eq!(block.push(&[index]).is_ok(), valid);
            assert_eq!(block.finish().is_ok(), valid);
        }
        for representation in [0x00, 0x10] {
            let mut wire = vec![0x88];
            new_field(&mut wire, representation, b"x", b"a");
            let mut block = ResponseBlock::new();
            block.push(&wire).unwrap();
            assert!(block.push(&[0xbe]).is_err());
        }
        let mut wire = vec![0x20, 0x88];
        new_field(&mut wire, 0x40, b"x", b"a");
        let mut block = ResponseBlock::new();
        block.push(&wire).unwrap();
        assert!(block.push(&[0xbe]).is_err());
    }

    #[test]
    fn malformed_indexes_lengths_updates_and_partial_blocks_fail_closed() {
        for wire in [
            &[0x80][..],
            &[0xbe],
            &[0x88, 0x20],
            &[0x3f, 0xe2, 0x1f],
            &[0x88, 0x40, 0],
            &[0x88, 0x40, 1, b'x', 0x81, 0xff],
            &[0x82],
        ] {
            let mut block = ResponseBlock::new();
            assert!(block.push(wire).is_err(), "{wire:?}");
            assert!(block.push(&[]).is_err());
            assert!(block.finish().is_err());
        }
        for wire in [
            &[][..],
            &[0xff],
            &[0x88, 0x40],
            &[0x88, 0x40, 1],
            &[0x88, 0x40, 1, b'x'],
            &[0x88, 0x40, 1, b'x', 2, b'a'],
        ] {
            let mut block = ResponseBlock::new();
            block.push(wire).unwrap();
            assert!(block.finish().is_err(), "{wire:?}");
        }
    }

    #[test]
    fn encoded_and_decoded_limits_are_independent_and_poison_failures() {
        let mut block = ResponseBlock::new();
        block.maximum_encoded_bytes = 1;
        block.push(&[0x88]).unwrap();
        block.finish().unwrap();
        let mut block = ResponseBlock::new();
        block.maximum_encoded_bytes = 1;
        assert!(block.push(&[0x88, 0x88]).is_err());
        assert!(block.finish().is_err());
        let maximum = HeadBounds::embedded().field_bytes as usize;
        for length in [maximum - 1, maximum] {
            let mut wire = vec![0x88];
            new_field(&mut wire, 0x00, b"x", &vec![b'a'; length]);
            let mut block = ResponseBlock::new();
            let result = block.push(&wire);
            assert_eq!(result.is_ok(), length < maximum);
            assert_eq!(block.finish().is_ok(), length < maximum);
        }
    }

    #[test]
    fn oversized_indexed_fields_clear_the_table_without_unbounded_capture() {
        let mut wire = vec![0x88];
        new_field(&mut wire, 0x40, b"x", b"a");
        let oversized = vec![b'b'; MAXIMUM_DYNAMIC_TABLE_BYTES as usize - 32];
        new_field(&mut wire, 0x40, b"y", &oversized);
        let mut block = ResponseBlock::new();
        block.push(&wire).unwrap();
        assert!(block.capture.is_none());
        assert!(block.table.lookup(62).is_err());
        assert_eq!(format!("{block:?}"), "ResponseBlock([redacted])");
        block.finish().unwrap();
    }
}
