//! Where an answer is carried, at every boundary that decides it.
//!
//! Byte boundaries are asserted one under, exactly at, and one over, because
//! every off-by-one in a bound lives in exactly those three places and nowhere
//! else. The sums are asserted too: a message is an envelope, that envelope
//! again as escaped text, an optional address, and an era's decoration, and a
//! budget that only bounded the first would hold right up until an answer was
//! interesting.

use std::path::PathBuf;

use slingshot_command_line::machine_outcome_envelope::MAXIMUM_MACHINE_OUTCOME_ENVELOPE_BYTES;
use slingshot_command_line::model_context_protocol::size_budget::{
    Carriage, MAXIMUM_ADDRESS_BYTES, MAXIMUM_DECORATION_BYTES, PINNED_ACKNOWLEDGEMENT_CAP,
    WORST_CASE_ESCAPE_FACTOR, WORST_CASE_MESSAGE_BYTES, carriage_of, is_carriable,
    worst_case_message_of,
};
use slingshot_command_line::model_context_protocol::standard_stream_transport::{
    MAXIMUM_QUEUED_BYTES, maximum_line_bytes,
};

/// Where the byte boundaries live.
const FIXTURE: &str =
    "../slingshot-test-support/fixtures/model-context-protocol/size-budget/boundaries.jsonl";

/// One declared boundary.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Boundary {
    /// What it is called.
    name: String,
    /// How large the answer is.
    bytes: u64,
    /// Where an answer that size is carried.
    carriage: String,
}

/// Returns every declared boundary.
fn boundaries() -> Vec<Boundary> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(FIXTURE);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|failure| panic!("{} could not be read: {failure}", path.display()));
    text.lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .map(|line| serde_json::from_str(line).expect("every boundary reads"))
        .collect()
}

#[test]
fn every_declared_boundary_is_carried_where_the_fixture_says() {
    for boundary in boundaries() {
        let expected = match boundary.carriage.as_str() {
            "inline" => Carriage::Inline,
            _ => Carriage::Externalized,
        };
        assert_eq!(carriage_of(boundary.bytes), expected, "{}", boundary.name);
    }
}

#[test]
fn the_bound_is_inclusive_and_the_next_byte_is_not() {
    assert_eq!(carriage_of(MAXIMUM_MACHINE_OUTCOME_ENVELOPE_BYTES), Carriage::Inline);
    assert_eq!(carriage_of(MAXIMUM_MACHINE_OUTCOME_ENVELOPE_BYTES - 1), Carriage::Inline);
    assert_eq!(carriage_of(MAXIMUM_MACHINE_OUTCOME_ENVELOPE_BYTES + 1), Carriage::Externalized);
}

#[test]
fn an_envelope_leaves_room_for_the_message_that_carries_it() {
    assert_eq!(
        MAXIMUM_MACHINE_OUTCOME_ENVELOPE_BYTES.min(PINNED_ACKNOWLEDGEMENT_CAP),
        MAXIMUM_MACHINE_OUTCOME_ENVELOPE_BYTES,
        "an envelope larger than the cap could not be acknowledged at all"
    );
    assert_ne!(
        MAXIMUM_MACHINE_OUTCOME_ENVELOPE_BYTES, PINNED_ACKNOWLEDGEMENT_CAP,
        "an envelope exactly at the cap would leave nothing for its message"
    );
}

#[test]
fn the_worst_message_one_answer_produces_still_fits_what_is_written() {
    let worst = worst_case_message_of(MAXIMUM_MACHINE_OUTCOME_ENVELOPE_BYTES);
    assert_eq!(worst, WORST_CASE_MESSAGE_BYTES);
    assert!(worst < MAXIMUM_QUEUED_BYTES as u64, "the queue would refuse its own largest answer");
    assert!(
        worst < maximum_line_bytes() as u64,
        "the transport would refuse to write its own largest answer"
    );
}

#[test]
fn escaping_is_budgeted_at_its_worst_case_rather_than_its_usual_one() {
    let one = worst_case_message_of(1);
    let overhead = MAXIMUM_ADDRESS_BYTES + MAXIMUM_DECORATION_BYTES;
    assert!(one > overhead, "one byte of answer costs more than nothing");
    assert_eq!(
        one - overhead - MESSAGE_OVERHEAD,
        1 + WORST_CASE_ESCAPE_FACTOR,
        "a byte and its worst-case escape are both budgeted"
    );
}

/// What one message costs around its answer.
const MESSAGE_OVERHEAD: u64 = 512;

#[test]
fn an_answer_too_large_to_inline_is_carriable_because_it_is_addressed() {
    let enormous = u64::from(u32::MAX);
    assert_eq!(carriage_of(enormous), Carriage::Externalized);
    assert!(is_carriable(enormous), "an addressed answer costs an address rather than its bytes");
    assert!(is_carriable(MAXIMUM_MACHINE_OUTCOME_ENVELOPE_BYTES));
}
