//! Incremental checks agree with the established canonical byte codec.
use slingshot_domain::command::canonical_json_reader::{Bounds, require_canonical_reader};
use std::io::Read;

struct Chunks<'a> {
    bytes: &'a [u8],
    maximum: usize,
}
impl Read for Chunks<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let count = buffer.len().min(self.maximum).min(self.bytes.len());
        buffer[..count].copy_from_slice(&self.bytes[..count]);
        self.bytes = &self.bytes[count..];
        Ok(count)
    }
}

#[test]
fn partitions_preserve_canonical_spelling_and_refusals() {
    let bounds = Bounds { bytes: 4096, token_bytes: 512, depth: 32 };
    for text in [
        r#"{"a":[null,true,false,0,-1,"é","\\\""],"b":{"x":[]}}"#,
        "{}",
        "[]",
        "null",
        "0",
        r#""text""#,
        "",
        " {}",
        "{} ",
        "{}{}",
        "[1,]",
        "{\"a\":1,}",
        r#"{"b":0,"a":1}"#,
        r#"{"a":0,"a":1}"#,
        r#"{"a":{"z":0,"b":1}}"#,
        r#""\u0061""#,
        r#""\/""#,
        "01",
        "1.0",
        "-0",
        "[true false]",
        "\u{feff}{}",
    ] {
        let expected =
            slingshot_domain::command::canonical_json::require_canonical_bytes(text.as_bytes())
                .is_ok();
        for maximum in 1..=text.len().max(1) {
            assert_eq!(
                require_canonical_reader(Chunks { bytes: text.as_bytes(), maximum }, bounds)
                    .is_ok(),
                expected,
                "{text:?}, chunk {maximum}"
            );
        }
    }
}

#[test]
fn byte_token_and_depth_limits_are_inclusive() {
    let bounds = Bounds { bytes: 5, token_bytes: 3, depth: 1 };
    assert!(require_canonical_reader(b"[123]".as_slice(), bounds).is_ok());
    for text in ["[1234]", "[[]]", "[1]  ", "\"abcd\""] {
        assert!(require_canonical_reader(text.as_bytes(), bounds).is_err());
    }
    assert!(require_canonical_reader(b"[123]".as_slice(), Bounds { bytes: 4, ..bounds }).is_err());
    assert!(
        require_canonical_reader(b"[123]".as_slice(), Bounds { token_bytes: 2, ..bounds }).is_err()
    );
}

#[test]
fn large_arrays_are_validated_without_collecting_their_items() {
    // The source itself is incremental, too: a million scalar items never
    // exist in a backing Vec or String in this fixture.
    struct Items {
        remaining: usize,
        comma: bool,
    }
    impl Read for Items {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            if buffer.is_empty() || self.remaining == 0 {
                return Ok(0);
            }
            buffer[0] = if self.comma { b',' } else { b'0' };
            self.comma = !self.comma;
            self.remaining -= 1;
            Ok(1)
        }
    }
    let source =
        b"[".as_slice().chain(Items { remaining: 1_999_999, comma: false }).chain(b"]".as_slice());
    assert!(
        require_canonical_reader(source, Bounds { bytes: 2_000_001, token_bytes: 1, depth: 1 })
            .is_ok()
    );
}

#[test]
fn invalid_utf8_and_io_failures_never_produce_validation() {
    let bounds = Bounds { bytes: 100, token_bytes: 100, depth: 4 };
    assert!(require_canonical_reader(b"\"\xff\"".as_slice(), bounds).is_err());
    struct Broken;
    impl Read for Broken {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("private-canary"))
        }
    }
    let refusal = require_canonical_reader(b"[0".as_slice().chain(Broken), bounds).unwrap_err();
    assert!(!format!("{refusal:?} {refusal}").contains("private-canary"));
}
