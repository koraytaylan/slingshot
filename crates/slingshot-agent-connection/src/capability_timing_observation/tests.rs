//! Numeric grammar boundaries and observation isolation, without network or clock guesses.

use super::*;

const SAMPLE: &[u8] = b"cap_shape;dur=3,cap_identity;dur=17,cap_document;dur=5";
const ZERO: &[u8] = b"cap_shape;dur=0,cap_identity;dur=0,cap_document;dur=0";
const EXPECTED_DURATIONS: [u64; PHASE_COUNT] = [3, 17, 5];

fn headers(value: &[u8]) -> http::HeaderMap {
    let mut headers = http::HeaderMap::new();
    headers.append("server-timing", http::HeaderValue::from_bytes(value).unwrap());
    headers
}

#[test]
fn each_phase_keeps_its_integer_duration() {
    assert_eq!(parse_header(&headers(SAMPLE)), HeaderObservation::Parsed([3, 17, 5]));
}

#[test]
fn an_absent_header_does_not_mean_zero_cost() {
    assert_eq!(parse_header(&http::HeaderMap::new()), HeaderObservation::Absent);
    assert_eq!(parse_header(&headers(ZERO)), HeaderObservation::Parsed([0, 0, 0]));
}

#[test]
fn duplicate_headers_are_unusable_even_when_equal() {
    let mut headers = headers(SAMPLE);
    headers.append("server-timing", http::HeaderValue::from_bytes(SAMPLE).unwrap());
    assert_eq!(parse_header(&headers), HeaderObservation::Rejected);
}

#[test]
fn descriptions_and_unknown_metrics_never_become_observations() {
    for value in [
        b"cap_shape;dur=3;desc=private,cap_identity;dur=17,cap_document;dur=5".as_slice(),
        b"cap_shape;dur=3,cap_identity;dur=17,cap_document;dur=5,other;dur=1",
        b"cap_shape;dur=3,cap_document;dur=5,cap_identity;dur=17",
        b"cap_shape;dur=3,cap_identity;dur=17",
        b"Cap_shape;dur=3,cap_identity;dur=17,cap_document;dur=5",
    ] {
        assert_eq!(parse_header(&headers(value)), HeaderObservation::Rejected);
    }
}

#[test]
fn durations_reject_signs_fractions_leading_zeroes_and_exponents() {
    for duration in ["-1", "+1", "1.0", "01", "1e3", "", " 1", "1 "] {
        let value = format!("cap_shape;dur={duration},cap_identity;dur=0,cap_document;dur=0");
        assert_eq!(parse_header(&headers(value.as_bytes())), HeaderObservation::Rejected);
    }
}

#[test]
fn every_phase_accepts_the_duration_limit_and_refuses_one_more() {
    for position in [0, 1, 2] {
        for (duration, accepted) in
            [(MAXIMUM_DURATION_MILLISECONDS, true), (MAXIMUM_DURATION_MILLISECONDS + 1, false)]
        {
            let mut durations = [0; PHASE_COUNT];
            durations[position] = duration;
            let [shape, identity, document] = durations;
            let value = format!(
                "cap_shape;dur={shape},cap_identity;dur={identity},cap_document;dur={document}"
            );
            let expected = if accepted {
                HeaderObservation::Parsed(durations)
            } else {
                HeaderObservation::Rejected
            };
            assert_eq!(parse_header(&headers(value.as_bytes())), expected);
        }
    }
}

#[test]
fn the_header_byte_limit_is_inclusive() {
    for (length, accepted) in
        [(MAXIMUM_TIMING_HEADER_BYTES, true), (MAXIMUM_TIMING_HEADER_BYTES + 1, false)]
    {
        let padding = " ".repeat(length - ZERO.len());
        let value = format!("cap_shape;dur=0,{padding}cap_identity;dur=0,cap_document;dur=0");
        assert_eq!(value.len(), length);
        let expected = if accepted {
            HeaderObservation::Parsed([0, 0, 0])
        } else {
            HeaderObservation::Rejected
        };
        assert_eq!(parse_header(&headers(value.as_bytes())), expected);
    }
}

#[test]
fn only_separator_space_and_tab_are_accepted() {
    let value = b"cap_shape;dur=3, \tcap_identity;dur=17,\t cap_document;dur=5";
    assert_eq!(parse_header(&headers(value)), HeaderObservation::Parsed([3, 17, 5]));
    for value in [
        b" cap_shape;dur=3,cap_identity;dur=17,cap_document;dur=5".as_slice(),
        b"cap_shape;dur=3,cap_identity;dur=17,cap_document;dur=5 ",
        b"cap_shape;dur=3,\ncap_identity;dur=17,cap_document;dur=5",
        b"cap_shape;dur=3,cap_identity;dur=17,cap_document;dur=\xff",
    ] {
        assert_eq!(parse_value(value), None);
    }
}

#[test]
fn classifications_and_durations_are_counted_separately() {
    let counters = Counters::default();
    counters.observe(OBSERVED_STATUS, &http::HeaderMap::new());
    counters.observe(OBSERVED_STATUS, &headers(b"unusable"));
    counters.observe(OBSERVED_STATUS, &headers(SAMPLE));
    assert_eq!(
        counters.snapshot(),
        Snapshot {
            absent_headers: 1,
            rejected_headers: 1,
            parsed_headers: 1,
            shape_milliseconds: EXPECTED_DURATIONS[0],
            identity_milliseconds: EXPECTED_DURATIONS[1],
            document_milliseconds: EXPECTED_DURATIONS[2],
        }
    );
}

#[test]
fn other_statuses_are_not_capability_timing_observations() {
    let counters = Counters::default();
    for status in [202, 204, 400, 401, 403, 409, 500] {
        counters.observe(status, &headers(SAMPLE));
    }
    assert_eq!(counters.snapshot(), Counters::default().snapshot());
}

#[test]
fn independent_counters_do_not_share_request_history() {
    let first = Counters::default();
    let second = Counters::default();
    first.observe(OBSERVED_STATUS, &headers(SAMPLE));
    second.observe(OBSERVED_STATUS, &headers(ZERO));
    assert_eq!(first.snapshot().identity_milliseconds, EXPECTED_DURATIONS[1]);
    assert_eq!(second.snapshot().identity_milliseconds, 0);
    assert_eq!(first.snapshot().parsed_headers, 1);
    assert_eq!(second.snapshot().parsed_headers, 1);
}

#[test]
fn aggregate_counters_saturate_instead_of_wrapping() {
    let counter = AtomicU64::new(u64::MAX - 1);
    add(&counter, u64::MAX);
    assert_eq!(counter.load(Ordering::Relaxed), u64::MAX);
    add(&counter, 1);
    assert_eq!(counter.load(Ordering::Relaxed), u64::MAX);
}
