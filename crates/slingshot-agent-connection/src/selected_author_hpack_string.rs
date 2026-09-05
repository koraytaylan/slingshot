//! Length-delimited HPACK literal strings feeding a bounded decoded-octet sink.
//! This composes prefix integers and Huffman strings without collecting either
//! encoded or decoded strings. Emitted octets remain provisional until finish.

use crate::selected_author_hpack_huffman::{HuffmanRefusal, HuffmanString};
use crate::selected_author_hpack_integer::PrefixedInteger;

/// Invalid length, encoded string, usage bound, sink refusal or read state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the author HPACK literal string is invalid")]
pub struct StringRefusal;

/// One string's declared encoded length and optional streaming Huffman state.
pub struct LiteralString {
    length: PrefixedInteger,
    remaining: Option<u64>,
    huffman: Option<HuffmanString>,
    poisoned: bool,
}

impl core::fmt::Debug for LiteralString {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("LiteralString([redacted])")
    }
}

impl LiteralString {
    /// Consumes the first length octet under the enclosing encoded-block bound.
    /// The sink must separately impose decoded field and aggregate bounds.
    pub fn start(first: u8, maximum_encoded_bytes: u64) -> Result<Self, StringRefusal> {
        let length =
            PrefixedInteger::start(first, 7, maximum_encoded_bytes).map_err(|_| StringRefusal)?;
        Ok(Self {
            remaining: length.value(),
            length,
            huffman: (first & 0x80 != 0).then(HuffmanString::new),
            poisoned: false,
        })
    }

    /// Signals a proven string boundary without consuming the next instruction.
    pub fn is_complete(&self) -> bool {
        !self.poisoned && self.remaining == Some(0)
    }

    /// True only when a length continuation proved the encoded usage bound exceeded.
    pub(crate) fn encoded_limit_exceeded(&self) -> bool { self.length.limit_exceeded() }

    /// Remaining payload once the complete encoded length has been read.
    pub(crate) fn remaining_encoded_bytes(&self) -> Option<u64> { self.remaining }

    /// Consumes one length-continuation or string-data octet. Refusal prevents
    /// further decoding and finish; no caller can reinterpret it as raw text.
    pub fn push(
        &mut self,
        byte: u8,
        mut emit: impl FnMut(u8) -> Result<(), StringRefusal>,
    ) -> Result<(), StringRefusal> {
        if self.poisoned || self.is_complete() {
            self.poisoned = true;
            return Err(StringRefusal);
        }
        self.poisoned = true;
        if let Some(remaining) = self.remaining {
            if let Some(decoder) = &mut self.huffman {
                decoder
                    .push(byte, |symbol| emit(symbol).map_err(|_| HuffmanRefusal))
                    .map_err(|_| StringRefusal)?;
            } else {
                emit(byte)?;
            }
            self.remaining = Some(remaining - 1);
            if remaining == 1
                && let Some(decoder) = self.huffman.take()
            {
                decoder.finish().map_err(|_| StringRefusal)?;
            }
        } else {
            self.remaining = self.length.push(byte).map_err(|_| StringRefusal)?;
        }
        self.poisoned = false;
        Ok(())
    }

    /// Requires the declared length to be exhausted with valid Huffman padding.
    pub fn finish(self) -> Result<(), StringRefusal> {
        if !self.is_complete() {
            return Err(StringRefusal);
        }
        if let Some(decoder) = self.huffman {
            decoder.finish().map_err(|_| StringRefusal)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_strings_feed_the_incremental_decoded_header_gate() {
        use crate::selected_author_http2_headers::DecodedHeadReader;
        fn feed(wire: &[u8], mut emit: impl FnMut(u8) -> Result<(), StringRefusal>) {
            let mut literal = LiteralString::start(wire[0], 64).unwrap();
            for byte in &wire[1..] {
                literal.push(*byte, &mut emit).unwrap();
            }
            literal.finish().unwrap();
        }
        let mut headers = DecodedHeadReader::new();
        headers.begin_field().unwrap();
        feed(b"\x07:status", |byte| headers.name_byte(byte).map_err(|_| StringRefusal));
        headers.begin_value().unwrap();
        feed(b"\x03200", |byte| headers.value_byte(byte).map_err(|_| StringRefusal));
        headers.end_field().unwrap();
        headers.begin_field().unwrap();
        feed(&[0x88, 0x25, 0xa8, 0x49, 0xe9, 0x5b, 0xa9, 0x7d, 0x7f], |byte| {
            headers.name_byte(byte).map_err(|_| StringRefusal)
        });
        headers.begin_value().unwrap();
        feed(&[0x89, 0x25, 0xa8, 0x49, 0xe9, 0x5b, 0xb8, 0xe8, 0xb4, 0xbf], |byte| {
            headers.value_byte(byte).map_err(|_| StringRefusal)
        });
        headers.end_field().unwrap();
        let (status, fields) = headers.finish().unwrap().into_parts();
        assert_eq!(status, 200);
        assert_eq!(fields["custom-key"], "custom-value");
    }

    #[test]
    fn raw_and_huffman_literals_share_exact_boundaries() {
        for (wire, expected) in [
            (&[3, b'a', b'b', b'c'][..], b"abc".as_slice()),
            (
                &[0x8c, 0xf1, 0xe3, 0xc2, 0xe5, 0xf2, 0x3a, 0x6b, 0xa0, 0xab, 0x90, 0xf4, 0xff][..],
                b"www.example.com".as_slice(),
            ),
            (&[0][..], b"".as_slice()),
            (&[0x80][..], b"".as_slice()),
        ] {
            let mut string = LiteralString::start(wire[0], (wire.len() - 1) as u64).unwrap();
            let mut output = Vec::new();
            for byte in &wire[1..] {
                assert!(!string.is_complete());
                string
                    .push(*byte, |symbol| {
                        output.push(symbol);
                        Ok(())
                    })
                    .unwrap();
            }
            assert!(string.is_complete());
            assert_eq!(output, expected);
            string.finish().unwrap();
        }
    }

    #[test]
    fn multi_octet_lengths_refuse_over_budget_before_emitting_content() {
        let mut string = LiteralString::start(127, 130).unwrap();
        string.push(3, |_| panic!("length bytes are not content")).unwrap();
        for _ in 0..130 {
            string.push(b'x', |_| Ok(())).unwrap();
        }
        string.finish().unwrap();
        assert!(LiteralString::start(3, 2).is_err());
        let mut string = LiteralString::start(127, 129).unwrap();
        assert!(string.push(3, |_| panic!("oversized length must not emit")).is_err());
        assert!(!string.is_complete());
        assert!(string.finish().is_err());
    }

    #[test]
    fn truncation_bad_padding_sink_refusal_and_surplus_bytes_poison_strings() {
        assert!(LiteralString::start(127, 200).unwrap().finish().is_err());
        let mut partial = LiteralString::start(2, 2).unwrap();
        partial.push(b'x', |_| Ok(())).unwrap();
        assert!(partial.finish().is_err());
        let mut padding = LiteralString::start(0x81, 1).unwrap();
        assert!(padding.push(0xff, |_| Ok(())).is_err());
        assert!(padding.finish().is_err());
        for first in [1, 0x81] {
            let mut refused = LiteralString::start(first, 1).unwrap();
            assert!(refused.push(b'a', |_| Err(StringRefusal)).is_err());
            assert!(refused.push(0, |_| panic!("refused strings do not emit")).is_err());
            assert!(refused.finish().is_err());
        }
        let mut empty = LiteralString::start(0, 0).unwrap();
        assert!(empty.push(0, |_| panic!("next instruction is not string data")).is_err());
        assert_eq!(format!("{empty:?}"), "LiteralString([redacted])");
        assert!(empty.finish().is_err());
    }
}
