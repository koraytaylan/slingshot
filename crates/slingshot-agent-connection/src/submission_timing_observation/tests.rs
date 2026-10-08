//! Numeric grammar boundaries and observation isolation, without network or clock guesses.

use super::*;

const SAMPLE: &[u8] = b"admission;dur=3,execution;dur=17,persistence;dur=5";
const ZERO: &[u8] = b"admission;dur=0,execution;dur=0,persistence;dur=0";
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
        b"admission;dur=3;desc=private,execution;dur=17,persistence;dur=5".as_slice(),
        b"admission;dur=3,execution;dur=17,persistence;dur=5,other;dur=1",
        b"admission;dur=3,persistence;dur=5,execution;dur=17",
        b"admission;dur=3,execution;dur=17",
        b"Admission;dur=3,execution;dur=17,persistence;dur=5",
    ] {
        assert_eq!(parse_header(&headers(value)), HeaderObservation::Rejected);
    }
}

#[test]
fn durations_reject_signs_fractions_leading_zeroes_and_exponents() {
    for duration in ["-1", "+1", "1.0", "01", "1e3", "", " 1", "1 "] {
        let value = format!("admission;dur={duration},execution;dur=0,persistence;dur=0");
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
            let [admission, execution, persistence] = durations;
            let value = format!(
                "admission;dur={admission},execution;dur={execution},persistence;dur={persistence}"
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
        let value = format!("admission;dur=0,{padding}execution;dur=0,persistence;dur=0");
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
    let value = b"admission;dur=3, \texecution;dur=17,\t persistence;dur=5";
    assert_eq!(parse_header(&headers(value)), HeaderObservation::Parsed([3, 17, 5]));
    for value in [
        b" admission;dur=3,execution;dur=17,persistence;dur=5".as_slice(),
        b"admission;dur=3,execution;dur=17,persistence;dur=5 ",
        b"admission;dur=3,\nexecution;dur=17,persistence;dur=5",
        b"admission;dur=3,execution;dur=17,persistence;dur=\xff",
    ] {
        assert_eq!(parse_value(value), None);
    }
}

#[test]
fn classifications_and_durations_are_counted_separately() {
    let counters = Counters::default();
    counters.observe(ACCEPTED_STATUS, &http::HeaderMap::new());
    counters.observe(ACCEPTED_STATUS, &headers(b"unusable"));
    counters.observe(ACCEPTED_STATUS, &headers(SAMPLE));
    assert_eq!(
        counters.snapshot(),
        Snapshot {
            absent_headers: 1,
            rejected_headers: 1,
            parsed_headers: 1,
            admission_milliseconds: EXPECTED_DURATIONS[0],
            execution_milliseconds: EXPECTED_DURATIONS[1],
            persistence_milliseconds: EXPECTED_DURATIONS[2],
        }
    );
}

#[test]
fn other_statuses_are_not_submission_timing_observations() {
    let counters = Counters::default();
    for status in [200, 204, 400, 401, 403, 409, 500] {
        counters.observe(status, &headers(SAMPLE));
    }
    assert_eq!(counters.snapshot(), Counters::default().snapshot());
}

#[test]
fn independent_counters_do_not_share_request_history() {
    let first = Counters::default();
    let second = Counters::default();
    first.observe(ACCEPTED_STATUS, &headers(SAMPLE));
    second.observe(ACCEPTED_STATUS, &headers(ZERO));
    assert_eq!(first.snapshot().execution_milliseconds, EXPECTED_DURATIONS[1]);
    assert_eq!(second.snapshot().execution_milliseconds, 0);
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
