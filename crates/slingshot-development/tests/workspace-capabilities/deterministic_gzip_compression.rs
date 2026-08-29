//! Probe for the deterministic compression capability.
//!
//! Requires a compressor whose header carries no wall-clock timestamp and no
//! ambient operating-system label, so two runs over the same bytes produce the
//! same output and the output decompresses to the original bytes.

use std::io::{Read, Write};

use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;

/// Fixed modification time every deterministic member carries.
const NORMALIZED_MODIFICATION_TIME: u32 = 0;

/// Operating-system label that identifies no ambient host.
const UNKNOWN_OPERATING_SYSTEM: u8 = 255;

/// Compresses the same input under the deterministic policy.
fn compress(input: &[u8]) -> Vec<u8> {
    let header = flate2::GzBuilder::new()
        .mtime(NORMALIZED_MODIFICATION_TIME)
        .operating_system(UNKNOWN_OPERATING_SYSTEM);
    let mut encoder = header.write(Vec::new(), Compression::best());
    encoder.write_all(input).expect("the input compresses");
    encoder.finish().expect("the member finishes")
}

#[test]
fn two_runs_over_the_same_bytes_produce_the_same_compressed_member() {
    let input = b"slingshot release artifact bytes".repeat(16);
    let first = compress(&input);
    let second = compress(&input);
    assert_eq!(first, second, "the member is byte-identical across runs");
    assert!(first.len() < input.len(), "the member is smaller than its input");

    let mut restored = Vec::new();
    GzDecoder::new(first.as_slice()).read_to_end(&mut restored).expect("the member decompresses");
    assert_eq!(restored, input);

    let mut corrupted = first.clone();
    let last = corrupted.len() - 1;
    corrupted[last] ^= 0xff;
    let mut refused = Vec::new();
    assert!(
        GzDecoder::new(corrupted.as_slice()).read_to_end(&mut refused).is_err(),
        "a corrupt member must be refused"
    );

    let separate = GzEncoder::new(Vec::new(), Compression::best());
    assert!(!separate.finish().expect("an empty member finishes").is_empty());
}
