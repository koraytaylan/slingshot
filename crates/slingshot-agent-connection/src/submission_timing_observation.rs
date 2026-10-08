//! Fixed-size numeric observations of the agent's optional submission timing header.
//!
//! Only fully validated finite responses with status 202 are considered. These
//! observations do not validate a submission acknowledgement or prove a terminal
//! command outcome. No header text, origin, request identity or individual history
//! is retained. Malformed optional timing changes only its observation category.
//!
//! Snapshots are cumulative, independently atomic and saturating. Compare them at
//! quiescent boundaries in a dedicated sequential probe. Durations are server-side
//! integer milliseconds; they exclude platform queueing, authentication, body
//! intake, response writing and network time, and do not explain those costs.

use http::HeaderMap;
use std::sync::atomic::{AtomicU64, Ordering};

const PHASE_COUNT: usize = 3;
const STATE_COUNT: usize = 3;
const ACCEPTED_STATUS: u16 = http::StatusCode::ACCEPTED.as_u16();
const MAXIMUM_TIMING_HEADER_BYTES: usize = 99;
const MAXIMUM_DURATION_MILLISECONDS: u64 = 9_223_372_036_854;
const DECIMAL_RADIX: u64 = 10;

/// Payload-free aggregate counts and sums, sampled independently.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct Snapshot {
    /// Validated status-202 responses without this optional header.
    pub absent_headers: u64,
    /// Validated status-202 responses whose optional header is unusable.
    pub rejected_headers: u64,
    /// Validated status-202 responses with exactly the bounded numeric grammar.
    pub parsed_headers: u64,
    /// Saturating sum of admission durations from parsed headers.
    pub admission_milliseconds: u64,
    /// Saturating sum of execution durations from parsed headers.
    pub execution_milliseconds: u64,
    /// Saturating sum of persistence durations from parsed headers.
    pub persistence_milliseconds: u64,
}

#[derive(Default)]
struct Counters {
    states: [AtomicU64; STATE_COUNT],
    durations: [AtomicU64; PHASE_COUNT],
}

#[derive(Debug, PartialEq, Eq)]
enum HeaderObservation {
    Absent,
    Rejected,
    Parsed([u64; PHASE_COUNT]),
}

static COUNTERS: std::sync::LazyLock<Counters> = std::sync::LazyLock::new(Counters::default);

/// Samples process-wide numeric observations without retaining remote text.
///
/// Concurrent exchanges can span a snapshot. A parsed header remains diagnostic
/// data, not acknowledgement, physical-job or terminal-outcome evidence.
#[must_use]
pub fn snapshot() -> Snapshot {
    COUNTERS.snapshot()
}

/// Called only after the common finite-response gate accepts every head and body bound.
pub(crate) fn record(status: u16, headers: &HeaderMap) {
    COUNTERS.observe(status, headers);
}

impl Counters {
    fn observe(&self, status: u16, headers: &HeaderMap) {
        if status != ACCEPTED_STATUS {
            return;
        }
        match parse_header(headers) {
            HeaderObservation::Absent => add(&self.states[0], 1),
            HeaderObservation::Rejected => add(&self.states[1], 1),
            HeaderObservation::Parsed(durations) => {
                for (counter, duration) in self.durations.iter().zip(durations) {
                    add(counter, duration);
                }
                add(&self.states[2], 1);
            }
        }
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            absent_headers: self.states[0].load(Ordering::Relaxed),
            rejected_headers: self.states[1].load(Ordering::Relaxed),
            parsed_headers: self.states[2].load(Ordering::Relaxed),
            admission_milliseconds: self.durations[0].load(Ordering::Relaxed),
            execution_milliseconds: self.durations[1].load(Ordering::Relaxed),
            persistence_milliseconds: self.durations[2].load(Ordering::Relaxed),
        }
    }
}

fn add(counter: &AtomicU64, amount: u64) {
    if amount == 0 {
        return;
    }
    let _previous = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |held| {
        Some(held.saturating_add(amount))
    });
}

fn parse_header(headers: &HeaderMap) -> HeaderObservation {
    let mut values = headers.get_all("server-timing").iter();
    let Some(value) = values.next() else {
        return HeaderObservation::Absent;
    };
    if values.next().is_some() {
        return HeaderObservation::Rejected;
    }
    match parse_value(value.as_bytes()) {
        Some(durations) => HeaderObservation::Parsed(durations),
        None => HeaderObservation::Rejected,
    }
}

fn parse_value(value: &[u8]) -> Option<[u64; PHASE_COUNT]> {
    if value.len() > MAXIMUM_TIMING_HEADER_BYTES {
        return None;
    }
    let mut fields = value.split(|byte| *byte == b',');
    let admission = parse_duration(fields.next()?, b"admission;dur=")?;
    let execution = parse_duration(separator_whitespace(fields.next()?), b"execution;dur=")?;
    let persistence = parse_duration(separator_whitespace(fields.next()?), b"persistence;dur=")?;
    fields.next().is_none().then_some([admission, execution, persistence])
}

fn separator_whitespace(mut value: &[u8]) -> &[u8] {
    while matches!(value.first(), Some(b' ' | b'\t')) {
        value = &value[1..];
    }
    value
}

fn parse_duration(field: &[u8], prefix: &[u8]) -> Option<u64> {
    let digits = field.strip_prefix(prefix)?;
    let first = digits.first()?;
    if *first == b'0' && digits.len() != 1 {
        return None;
    }
    let mut duration = 0_u64;
    for digit in digits {
        if !digit.is_ascii_digit() {
            return None;
        }
        duration = duration.checked_mul(DECIMAL_RADIX)?.checked_add(u64::from(*digit - b'0'))?;
    }
    (duration <= MAXIMUM_DURATION_MILLISECONDS).then_some(duration)
}

#[cfg(test)]
#[path = "submission_timing_observation/tests.rs"]
mod tests;
