//! Process-local retry deadlines reconstructed from durable observations once.
//!
//! Later wall-clock corrections cannot move an existing deadline. A restarted
//! process clamps elapsed wall evidence to the original delay, so the longest
//! reconstructed wait is the delay the retry originally chose.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;
use tokio::time::Instant;

use slingshot_domain::operation::RecoveryFact;
use slingshot_storage::operation::scheduler_claim::QueuedCandidate;

#[cfg(test)]
mod tests;

/// Identifies a durable delay independently of unrelated progress revisions.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Observation {
    /// Original persisted wall-clock sample.
    observed: u64,
    /// Maximum elapsed duration for this retry.
    delay: u64,
    /// Distinguishes successive retries observed in the same clock tick.
    attempt: u64,
}

impl Observation {
    /// Reads the timing evidence of one retained recovery.
    pub(crate) fn of(recovery: &RecoveryFact) -> Self {
        Self {
            observed: recovery.retry_observed_at_unix_milliseconds,
            delay: recovery.retry_delay_milliseconds,
            attempt: u64::from(recovery.attempt_count),
        }
    }
}

/// A retry's monotonic wait, bounded by its original durable delay.
struct Deadline {
    /// Which retry this deadline represents.
    observation: Observation,
    /// Monotonic reading at reconstruction.
    started: Instant,
    /// Remaining duration reconstructed exactly once.
    remaining: Duration,
}

/// Deadlines for the bounded pending queue, pruned on each selection snapshot.
#[derive(Default)]
pub(crate) struct RetrySchedule {
    /// One live deadline per operation in the selected target.
    deadlines: BTreeMap<String, Deadline>,
}

impl RetrySchedule {
    /// Reconstructs all newly observed retries and chooses the first ready row.
    pub(crate) fn select(
        &mut self,
        candidates: &[QueuedCandidate],
        wall: u64,
        monotonic: Instant,
    ) -> Option<usize> {
        let retained: BTreeSet<&str> =
            candidates.iter().map(|candidate| candidate.operation_identifier.as_str()).collect();
        self.deadlines.retain(|identifier, _| retained.contains(identifier.as_str()));
        let mut selected = None;
        for (index, candidate) in candidates.iter().enumerate() {
            let observation = Observation {
                observed: candidate.retry_observed_at_unix_milliseconds,
                delay: candidate.retry_delay_milliseconds,
                attempt: candidate.attempt_count,
            };
            if self.ready(&candidate.operation_identifier, observation, wall, monotonic)
                && selected.is_none()
            {
                selected = Some(index);
            }
        }
        selected
    }

    /// Uses wall evidence only on the first observation of a particular retry.
    pub(crate) fn ready(
        &mut self,
        identifier: &str,
        observation: Observation,
        wall: u64,
        monotonic: Instant,
    ) -> bool {
        if !self
            .deadlines
            .get(identifier)
            .is_some_and(|deadline| deadline.observation == observation)
        {
            let elapsed = wall.saturating_sub(observation.observed).min(observation.delay);
            self.deadlines.insert(
                identifier.to_owned(),
                Deadline {
                    observation,
                    started: monotonic,
                    remaining: Duration::from_millis(observation.delay.saturating_sub(elapsed)),
                },
            );
        }
        self.deadlines.get(identifier).is_some_and(|deadline| {
            monotonic.saturating_duration_since(deadline.started) >= deadline.remaining
        })
    }

    /// Carries the scheduler's elapsed delay into an admitted execution.
    pub(crate) fn admitted(
        &mut self,
        identifier: &str,
        observation: Observation,
        monotonic: Instant,
    ) {
        self.deadlines.insert(
            identifier.to_owned(),
            Deadline { observation, started: monotonic, remaining: Duration::ZERO },
        );
    }
}
