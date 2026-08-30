//! The frame boundary, proved before anything reads a field.
//!
//! Two properties matter more than the rest. A prefix declaring more than the
//! contract allows is refused while the buffer still holds only the prefix, so
//! a peer cannot make this process allocate by lying about what it is about to
//! send - the test asserts the refusal happens with the payload absent, not
//! merely that an oversized payload is rejected once received.
//!
//! And a reader that has refused a frame refuses everything after it. Once a
//! frame is malformed the two sides no longer agree where the next one starts,
//! so continuing to read is how one bad frame becomes a stream of plausible
//! nonsense.

use serde_json::Value;
use slingshot_local_protocol::foundation_contract::FoundationContract;
use slingshot_local_protocol::framing::{
    FrameProgress, FrameReader, FramingFailure, progress, render, write_frame,
};

/// Payload vectors this test reads.
const PAYLOADS: &str = include_str!("fixtures/framing/payloads.jsonl");

/// Malformed vectors this test reads.
const MALFORMED: &str = include_str!("fixtures/framing/malformed.jsonl");

/// Fragmentation vectors this test reads.
const FRAGMENTS: &str = include_str!("fixtures/framing/fragments.jsonl");

/// Boundary vectors this test reads.
const BOUNDARIES: &str = include_str!("fixtures/framing/boundaries.jsonl");

/// Reads one row's string member.
fn text<'row>(row: &'row Value, member: &str) -> &'row str {
    row[member].as_str().unwrap_or_else(|| panic!("{member} is a string in {row}"))
}

/// Returns every row of one fixture.
fn rows(fixture: &str) -> Vec<Value> {
    fixture
        .lines()
        .map(|line| serde_json::from_str(line).expect("every fixture line is one object"))
        .collect()
}

/// Returns the bytes one hexadecimal member spells.
fn octets(row: &Value, member: &str) -> Vec<u8> {
    /// Radix the fixture writes bytes in.
    const HEXADECIMAL_RADIX: u32 = 16;
    /// Characters one byte occupies.
    const CHARACTERS_PER_OCTET: usize = 2;

    let spelling = text(row, member);
    (0..spelling.len() / CHARACTERS_PER_OCTET)
        .map(|position| {
            let pair = &spelling[position * CHARACTERS_PER_OCTET
                ..position * CHARACTERS_PER_OCTET + CHARACTERS_PER_OCTET];
            u8::from_str_radix(pair, HEXADECIMAL_RADIX).expect("a hexadecimal pair")
        })
        .collect()
}

/// Returns the framing limits the embedded contract declares.
fn limits() -> slingshot_local_protocol::foundation_contract::FramingLimits {
    FoundationContract::embedded().framing
}

#[test]
fn every_payload_vector_round_trips_byte_for_byte() {
    let vectors = rows(PAYLOADS);
    assert!(vectors.len() >= 5, "including the payload at the contract's maximum");
    for row in &vectors {
        let payload = text(row, "payload").as_bytes();
        let note = text(row, "note");
        let framed =
            render(&limits(), payload).unwrap_or_else(|failure| panic!("{note}: {failure}"));
        assert_eq!(framed, octets(row, "frame"), "{note}: framed differently");
        let mut reader = FrameReader::new();
        let read = reader
            .absorb(&limits(), &framed)
            .unwrap_or_else(|failure| panic!("{note}: {failure}"))
            .unwrap_or_else(|| panic!("{note}: a whole frame yielded nothing"));
        assert_eq!(read, payload, "{note}: the payload changed on the way through");
        assert_eq!(reader.buffered(), 0, "{note}: bytes were left behind");
    }
}

#[test]
fn an_oversized_prefix_is_refused_before_any_payload_byte_exists() {
    for row in rows(MALFORMED).iter().filter(|row| text(row, "reason") == "PayloadTooLarge") {
        let prefix = octets(row, "frame");
        let note = text(row, "note");
        assert_eq!(
            prefix.len(),
            limits().length_prefix_bytes as usize,
            "{note}: the vector delivers the prefix alone, and nothing after it"
        );
        let mut reader = FrameReader::new();
        let outcome = reader.absorb(&limits(), &prefix);
        assert!(
            matches!(outcome, Err(FramingFailure::PayloadTooLarge { .. })),
            "{note}: answered {outcome:?}"
        );
        assert!(reader.is_poisoned(), "{note}: the reader carried on");
    }
}

#[test]
fn every_malformed_vector_is_refused_for_its_own_reason() {
    let vectors = rows(MALFORMED);
    assert!(vectors.len() >= 7, "every distinct way a frame can be wrong");
    for row in &vectors {
        let note = text(row, "note");
        let reason = text(row, "reason");
        let mut reader = FrameReader::new();
        let outcome = reader.absorb(&limits(), &octets(row, "frame"));
        let named = match &outcome {
            Err(FramingFailure::PayloadTooLarge { .. }) => "PayloadTooLarge",
            Err(FramingFailure::NotText(_)) => "NotText",
            Err(FramingFailure::Unfinished) => "Unfinished",
            Err(FramingFailure::NoValue) => "NoValue",
            Err(FramingFailure::TrailingValue) => "TrailingValue",
            Err(FramingFailure::NestingTooDeep { .. }) => "NestingTooDeep",
            Err(FramingFailure::CollectionTooLarge { .. }) => "CollectionTooLarge",
            other => panic!("{note}: answered {other:?}"),
        };
        assert_eq!(named, reason, "{note}");
    }
}

#[test]
fn a_reader_that_refused_a_frame_reads_nothing_after_it() {
    let mut reader = FrameReader::new();
    let unfinished = b"{\"a\":1";
    let mut frame = (unfinished.len() as u32).to_be_bytes().to_vec();
    frame.extend_from_slice(unfinished);
    assert_eq!(
        reader.absorb(&limits(), &frame),
        Err(FramingFailure::Unfinished),
        "the frame ends inside an object"
    );
    assert!(reader.is_poisoned());

    let following = render(&limits(), b"{\"a\":1}").expect("a good frame");
    assert_eq!(
        reader.absorb(&limits(), &following),
        Err(FramingFailure::ReaderPoisoned),
        "the bytes after a malformed frame belong to nobody"
    );
    assert_eq!(reader.progress(&limits()), Err(FramingFailure::ReaderPoisoned));

    let mut fresh = FrameReader::new();
    assert!(
        fresh.absorb(&limits(), &following).expect("a good frame").is_some(),
        "and a new connection is unaffected"
    );
}

#[test]
fn every_fragmentation_point_reports_exact_progress() {
    let vectors = rows(FRAGMENTS);
    assert!(vectors.len() >= 7, "empty, mid-prefix, exact prefix, mid-payload, and whole");
    for row in &vectors {
        let note = text(row, "note");
        let first = octets(row, "first");
        let second = octets(row, "second");
        let mut reader = FrameReader::new();
        let held = {
            let mut watching = FrameReader::new();
            watching
                .absorb(&limits(), &first)
                .unwrap_or_else(|failure| panic!("{note}: {failure}"));
            watching.progress(&limits()).unwrap_or_else(|failure| panic!("{note}: {failure}"))
        };
        let early =
            reader.absorb(&limits(), &first).unwrap_or_else(|failure| panic!("{note}: {failure}"));
        let prefix = limits().length_prefix_bytes as usize;
        match held {
            FrameProgress::Empty => assert!(
                first.is_empty() || early.is_some(),
                "{note}: nothing is held only before a byte arrives or after a frame leaves"
            ),
            FrameProgress::PartialPrefix { received } => {
                assert_eq!(received, first.len(), "{note}");
                assert!(received < prefix, "{note}");
            }
            FrameProgress::PartialPayload { received, declared } => {
                assert_eq!(received, first.len() - prefix, "{note}");
                assert!(received < declared, "{note}");
            }
            FrameProgress::Complete { .. } => {
                panic!("{note}: a complete frame is consumed rather than held")
            }
        }
        if early.is_none() {
            let payload = reader
                .absorb(&limits(), &second)
                .unwrap_or_else(|failure| panic!("{note}: {failure}"))
                .unwrap_or_else(|| panic!("{note}: the second piece completed nothing"));
            assert_eq!(payload, text(row, "payload").as_bytes(), "{note}");
        }
        assert_eq!(reader.buffered(), 0, "{note}: bytes were left behind");
    }
}

#[test]
fn one_byte_either_side_of_the_maximum_is_the_boundary() {
    for row in rows(BOUNDARIES) {
        let declared = row["declared"].as_u64().expect("a declared length");
        let note = text(&row, "note");
        let accepted = row["accepted"].as_bool().expect("a verdict");
        let prefix = limits().length_prefix_bytes as usize;
        let mut frame = declared.to_be_bytes()[core::mem::size_of::<u64>() - prefix..].to_vec();
        let outcome = progress(&limits(), &frame);
        assert_eq!(outcome.is_ok(), accepted, "{note}: {outcome:?}");
        if !accepted {
            frame.clear();
            continue;
        }
        assert!(
            matches!(
                outcome.expect("accepted"),
                FrameProgress::PartialPayload { received: 0, .. } | FrameProgress::Complete { .. }
            ),
            "{note}: a prefix alone declares a payload still to come"
        );
    }
}

#[test]
fn a_transport_that_fails_partway_is_a_structured_error_and_not_a_panic() {
    /// A sink that refuses after the first byte.
    struct ShortSink {
        /// Bytes it has already accepted.
        accepted: usize,
    }

    impl std::io::Write for ShortSink {
        fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
            if self.accepted > 0 {
                return Err(std::io::Error::other("the peer went away"));
            }
            self.accepted += buffer.len().min(1);
            Ok(self.accepted)
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    let mut sink = ShortSink { accepted: 0 };
    let outcome = write_frame(&limits(), b"{\"a\":1}", &mut sink);
    assert!(
        matches!(outcome, Err(FramingFailure::TransportFailed(_))),
        "a half-written frame is a failure, not a shorter frame: {outcome:?}"
    );

    let mut accepted = Vec::new();
    write_frame(&limits(), b"{\"a\":1}", &mut accepted).expect("a sink that accepts");
    assert_eq!(accepted, render(&limits(), b"{\"a\":1}").expect("a frame"));
}

#[test]
fn the_codec_knows_nothing_about_operations_or_daemons() {
    let source = include_str!("../src/framing.rs");
    for absent in ["operation", "daemon", "namespace", "runtime_contract", "Execute"] {
        assert!(!source.contains(absent), "the codec mentions {absent}, which is a layer above it");
    }
}
