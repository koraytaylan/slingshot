//! Streaming HPACK Huffman strings (RFC 7541 section 5.2 and Appendix B).
//!
//! Decoded octets go directly to a fallible sink; no decoded string is
//! collected here. The fixed protocol trie is built at compile time and work
//! is linear in encoded bits. The enclosing HPACK parser supplies string
//! length boundaries and the header reader supplies decoded byte limits.

use crate::selected_author_hpack_codes::CODES;

const NO_SYMBOL: u16 = u16::MAX;

#[derive(Clone, Copy)]
struct Node {
    children: [usize; 2],
    symbol: u16,
}

const fn tree() -> [Node; 513] {
    let mut nodes = [Node { children: [0; 2], symbol: NO_SYMBOL }; 513];
    let mut used = 1;
    let mut symbol = 0;
    while symbol < CODES.len() {
        let (length, code) = CODES[symbol];
        let mut position = 0;
        let mut bit = length;
        while bit > 0 {
            assert!(nodes[position].symbol == NO_SYMBOL);
            bit -= 1;
            let branch = ((code >> bit) & 1) as usize;
            if nodes[position].children[branch] == 0 {
                assert!(used < nodes.len());
                nodes[position].children[branch] = used;
                used += 1;
            }
            position = nodes[position].children[branch];
        }
        assert!(nodes[position].symbol == NO_SYMBOL);
        assert!(nodes[position].children[0] == 0 && nodes[position].children[1] == 0);
        nodes[position].symbol = symbol as u16;
        symbol += 1;
    }
    assert!(used == nodes.len());
    nodes
}

static TREE: [Node; 513] = tree();

/// A malformed string or refused decoded octet, without private wire data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the author HPACK Huffman string is invalid")]
pub struct HuffmanRefusal;

/// One encoded string's state, reusable across any byte-chunk boundaries.
pub struct HuffmanString {
    position: usize,
    pending_bits: u8,
    pending_ones: bool,
    poisoned: bool,
}

impl core::fmt::Debug for HuffmanString {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("HuffmanString([redacted])")
    }
}

impl Default for HuffmanString {
    fn default() -> Self {
        Self::new()
    }
}

impl HuffmanString {
    /// Starts one HPACK string; there is no dynamic table or previous-string
    /// state in a Huffman string decoder.
    pub fn new() -> Self {
        Self { position: 0, pending_bits: 0, pending_ones: true, poisoned: false }
    }

    /// Sends each decoded octet to the bounded sink before reading more bits.
    /// A sink refusal permanently invalidates this string, including finish.
    pub fn push(
        &mut self,
        byte: u8,
        mut emit: impl FnMut(u8) -> Result<(), HuffmanRefusal>,
    ) -> Result<(), HuffmanRefusal> {
        if self.poisoned {
            return Err(HuffmanRefusal);
        }
        self.poisoned = true;
        for bit in (0..8).rev() {
            let branch = usize::from((byte >> bit) & 1);
            self.position = TREE[self.position].children[branch];
            if self.position == 0 {
                return Err(HuffmanRefusal);
            }
            self.pending_bits += 1;
            self.pending_ones &= branch == 1;
            let symbol = TREE[self.position].symbol;
            if symbol == 256 {
                return Err(HuffmanRefusal);
            }
            if symbol != NO_SYMBOL {
                emit(symbol as u8)?;
                self.position = 0;
                self.pending_bits = 0;
                self.pending_ones = true;
            }
        }
        self.poisoned = false;
        Ok(())
    }

    /// At the declared string boundary, only zero to seven EOS-prefix padding
    /// bits may remain. Literal EOS and overlong/non-EOS padding are errors.
    pub fn finish(self) -> Result<(), HuffmanRefusal> {
        if self.poisoned || self.pending_bits > 7 || !self.pending_ones {
            Err(HuffmanRefusal)
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hexadecimal(text: &str) -> Vec<u8> {
        text.as_bytes()
            .chunks_exact(2)
            .map(|pair| u8::from_str_radix(core::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect()
    }

    fn decode(bytes: &[u8]) -> Result<Vec<u8>, HuffmanRefusal> {
        let mut decoder = HuffmanString::new();
        let mut output = Vec::new();
        for byte in bytes {
            decoder.push(*byte, |symbol| {
                output.push(symbol);
                Ok(())
            })?;
        }
        decoder.finish()?;
        Ok(output)
    }

    fn encode(bytes: &[u8]) -> Vec<u8> {
        let mut output = Vec::new();
        let mut pending = 0;
        let mut length = 0;
        for byte in bytes {
            let (bits, code) = CODES[usize::from(*byte)];
            for bit in (0..bits).rev() {
                pending = (pending << 1) | ((code >> bit) & 1) as u8;
                length += 1;
                if length == 8 {
                    output.push(pending);
                    pending = 0;
                    length = 0;
                }
            }
        }
        if length != 0 {
            output.push((pending << (8 - length)) | ((1 << (8 - length)) - 1));
        }
        output
    }

    #[test]
    fn independent_rfc_examples_decode_across_encoded_octet_boundaries() {
        for (wire, plain) in [
            ("f1e3c2e5f23a6ba0ab90f4ff", "www.example.com"),
            ("a8eb10649cbf", "no-cache"),
            ("25a849e95ba97d7f", "custom-key"),
            ("25a849e95bb8e8b4bf", "custom-value"),
        ] {
            assert_eq!(decode(&hexadecimal(wire)).unwrap(), plain.as_bytes());
        }
        assert_eq!(decode(&[]).unwrap(), b"");
    }

    #[test]
    fn every_octet_and_every_legal_padding_length_round_trips() {
        let alphabet: Vec<u8> = (0..=255).collect();
        assert_eq!(decode(&encode(&alphabet)).unwrap(), alphabet);
        let mut padding_lengths = std::collections::BTreeSet::new();
        for byte in 0..=255_u8 {
            let length = CODES[usize::from(byte)].0;
            padding_lengths.insert((8 - length % 8) % 8);
            assert_eq!(decode(&encode(&[byte])).unwrap(), [byte]);
        }
        assert_eq!(padding_lengths.len(), 8);
    }

    #[test]
    fn eos_non_eos_padding_and_overlong_padding_are_refused() {
        for wire in [&[0xff][..], &[0xff, 0xff, 0xff, 0xff], &[0x1e], &[0x00], &[0xfe]] {
            assert!(decode(wire).is_err(), "{wire:?}");
        }
        let mut decoder = HuffmanString::new();
        for _ in 0..3 {
            decoder.push(0xff, |_| Ok(())).unwrap();
        }
        assert!(decoder.push(0xff, |_| panic!("EOS must not be emitted")).is_err());
        assert!(decoder.push(0, |_| panic!("a failed decoder must not emit")).is_err());
        assert!(decoder.finish().is_err());
    }

    #[test]
    fn sink_refusal_stops_decoding_and_invalidates_finish() {
        let mut decoder = HuffmanString::new();
        let mut count = 0;
        let mut failed = false;
        for byte in encode(b"aaaaaa") {
            if decoder
                .push(byte, |_| {
                    count += 1;
                    if count > 1 { Err(HuffmanRefusal) } else { Ok(()) }
                })
                .is_err()
            {
                failed = true;
                break;
            }
        }
        assert!(failed);
        assert_eq!(count, 2);
        assert!(decoder.push(0, |_| panic!("sink refusal is permanent")).is_err());
        assert_eq!(format!("{decoder:?}"), "HuffmanString([redacted])");
        assert!(decoder.finish().is_err());
    }

    #[test]
    fn compressed_output_is_bounded_by_the_real_decoded_header_reader() {
        use crate::selected_author_http2_headers::DecodedHeadReader;
        let bound = crate::author_hypertext_transfer_protocol_policy::HeadBounds::embedded()
            .field_bytes as usize;
        for length in [bound - 1, bound] {
            let mut headers = DecodedHeadReader::new();
            headers.begin_field().unwrap();
            for byte in b":status" {
                headers.name_byte(*byte).unwrap();
            }
            headers.begin_value().unwrap();
            for byte in b"200" {
                headers.value_byte(*byte).unwrap();
            }
            headers.end_field().unwrap();
            headers.begin_field().unwrap();
            headers.name_byte(b'x').unwrap();
            headers.begin_value().unwrap();
            let encoded = encode(&vec![b'a'; length]);
            assert!(encoded.len() < bound);
            let mut decoder = HuffmanString::new();
            let mut rejected = false;
            for byte in encoded {
                if decoder
                    .push(byte, |symbol| headers.value_byte(symbol).map_err(|_| HuffmanRefusal))
                    .is_err()
                {
                    rejected = true;
                    break;
                }
            }
            assert_eq!(rejected, length == bound);
            if rejected {
                assert!(decoder.finish().is_err());
                assert!(headers.finish().is_err());
            } else {
                decoder.finish().unwrap();
                headers.end_field().unwrap();
                let (_, fields) = headers.finish().unwrap().into_parts();
                assert_eq!(fields["x"].as_bytes().len(), length);
            }
        }
    }
}
