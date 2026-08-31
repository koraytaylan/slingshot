//! Deciding what runs next, fairly and within bounds, from persisted facts alone.
//!
//! The scheduler is a pure function of what the repository holds. It reads a
//! snapshot of rows, applies the contract's bounds, and returns directives; it
//! runs no timer, advances no remote work, and holds no state between calls.
//! Two schedulers given the same snapshot return the same decisions, which is
//! what makes a restart indistinguishable from a tick.
//!
//! Fairness is round-robin across callers, and enqueue order within each one.
//! A caller that keeps work queued continuously therefore cannot keep another
//! caller waiting: it gets one turn per pass like everybody else. Within a
//! caller the order is the order things were asked for, because a client that
//! submitted A then B has every reason to expect A first.
//!
//! Time is an input rather than something this module reads. Retry eligibility
//! is a comparison against a supplied instant, and after a restart the
//! remaining delay is recomputed by clamping the elapsed wall time between zero
//! and the original delay. A clock that moved backwards therefore waits at most
//! what it would have; one that moved forwards makes the retry eligible
//! immediately. Neither can create duplicate work, because eligibility only
//! decides when a retry becomes allowed and a retry is safe whenever it
//! happens.

use std::collections::BTreeMap;

use slingshot_domain::daemon_runtime_contract::DaemonRuntimeContract;
use slingshot_domain::operation::{RecoveryFact, remaining_delay_milliseconds};

/// The bounds a scheduler is held to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SchedulerBounds {
    /// Operations that may be in flight across this namespace at once.
    pub global_in_flight: u64,
    /// Operations that may be waiting across this namespace at once.
    pub global_pending: u64,
    /// Operations one caller may have waiting at once.
    pub pending_per_caller: u64,
    /// Operations one tick may select.
    pub selections_per_tick: u64,
}

impl SchedulerBounds {
    /// Returns the bounds the embedded runtime contract names.
    ///
    /// Read rather than declared, and with no local default: a scheduler that
    /// could invent a bound the manifest did not name would admit different
    /// amounts of work depending on which build was running.
    #[must_use]
    pub fn embedded() -> Self {
        let contract = DaemonRuntimeContract::embedded();
        Self {
            global_in_flight: contract.limit("maximum_global_in_flight_operations"),
            global_pending: contract.limit("maximum_global_pending_operations"),
            pending_per_caller: contract.limit("maximum_pending_operations_per_caller"),
            selections_per_tick: contract.limit("maximum_scheduler_selections_per_tick"),
        }
    }
}

/// Why a scheduler refused to admit more work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum AdmissionRefusal {
    /// This namespace already has every operation waiting that it may.
    #[error("this namespace holds {held} waiting operations and may hold {limit}")]
    GlobalPending {
        /// How many are waiting.
        held: u64,
        /// How many may be.
        limit: u64,
    },
    /// This caller already has every operation waiting that it may.
    #[error("this caller holds {held} waiting operations and may hold {limit}")]
    CallerPending {
        /// How many it has waiting.
        held: u64,
        /// How many it may have.
        limit: u64,
    },
}

/// One operation as the scheduler sees it.
///
/// Everything here is persisted. Nothing is remembered between ticks, so a
/// scheduler that has just started and one that has been running for a week
/// decide identically given the same rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduledOperation {
    /// The partition it belongs to.
    pub author_target_identity_digest: String,
    /// Who asked for it, or nothing when no caller said.
    pub caller_identity: Option<String>,
    /// Where it sits in its partition's arrival order.
    pub enqueue_sequence: u64,
    /// The identifier its caller chose.
    pub operation_identifier: String,
    /// Whether a committed resume has already made it eligible.
    pub resume_committed: bool,
    /// What it is waiting on, when it is waiting on something.
    pub outstanding_recovery: Option<RecoveryFact>,
}

impl ScheduledOperation {
    /// Returns whether this operation may be selected at `now`.
    ///
    /// An operation waiting on nothing is eligible. One waiting on a recovery
    /// is eligible when a person has explicitly resumed it, or when its
    /// remaining delay has elapsed. The explicit resume wins over the clock,
    /// because a person who asked for a retry has said something the clock
    /// cannot say.
    #[must_use]
    pub fn is_eligible(&self, now_unix_milliseconds: u64) -> bool {
        let Some(recovery) = &self.outstanding_recovery else {
            return true;
        };
        self.resume_committed || remaining_delay_milliseconds(recovery, now_unix_milliseconds) == 0
    }
}

/// One operation the scheduler decided to start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartDirective {
    /// The partition it belongs to.
    pub author_target_identity_digest: String,
    /// The identifier its caller chose.
    pub operation_identifier: String,
}

/// What the scheduler observed when it was asked to decide.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchedulerObservation {
    /// Operations already running.
    pub in_flight: u64,
    /// Every operation waiting, in whatever order the repository returned them.
    pub waiting: Vec<ScheduledOperation>,
}

/// The scheduler, which decides and remembers nothing.
#[derive(Debug, Clone, Copy)]
pub struct OperationScheduler {
    /// The bounds it is held to.
    bounds: SchedulerBounds,
}

impl OperationScheduler {
    /// Returns a scheduler held to `bounds`.
    #[must_use]
    pub fn new(bounds: SchedulerBounds) -> Self {
        Self { bounds }
    }

    /// Returns the bounds this scheduler is held to.
    #[must_use]
    pub fn bounds(&self) -> SchedulerBounds {
        self.bounds
    }

    /// Requires room for one more waiting operation from `caller`.
    ///
    /// Asked before anything is written, so a refusal reports counts that are
    /// still true rather than counts a partial insert has already changed.
    ///
    /// # Errors
    ///
    /// Returns [`AdmissionRefusal`] naming which bound is full.
    pub fn require_room(
        &self,
        observation: &SchedulerObservation,
        caller_identity: Option<&str>,
    ) -> Result<(), AdmissionRefusal> {
        let waiting = u64::try_from(observation.waiting.len()).unwrap_or(u64::MAX);
        if waiting >= self.bounds.global_pending {
            return Err(AdmissionRefusal::GlobalPending {
                held: waiting,
                limit: self.bounds.global_pending,
            });
        }
        let held = observation
            .waiting
            .iter()
            .filter(|operation| operation.caller_identity.as_deref() == caller_identity)
            .count();
        let held = u64::try_from(held).unwrap_or(u64::MAX);
        if held >= self.bounds.pending_per_caller {
            return Err(AdmissionRefusal::CallerPending {
                held,
                limit: self.bounds.pending_per_caller,
            });
        }
        Ok(())
    }

    /// Returns what to start now, in the order to start it.
    ///
    /// One pass takes at most one operation from each caller before it takes a
    /// second from any, which is what stops a busy caller starving a quiet one.
    /// Passes repeat until a bound is reached or nothing eligible is left.
    #[must_use]
    pub fn select(
        &self,
        observation: &SchedulerObservation,
        now_unix_milliseconds: u64,
    ) -> Vec<StartDirective> {
        let mut queues = self.eligible_by_caller(observation, now_unix_milliseconds);
        let free = self.bounds.global_in_flight.saturating_sub(observation.in_flight);
        let allowed =
            usize::try_from(free.min(self.bounds.selections_per_tick)).unwrap_or_default();
        let mut selected = Vec::new();
        while selected.len() < allowed {
            let taken = selected.len();
            for queued in queues.values_mut() {
                if selected.len() >= allowed {
                    break;
                }
                if !queued.is_empty() {
                    selected.push(queued.remove(0));
                }
            }
            if selected.len() == taken {
                break;
            }
        }
        selected
    }

    /// Returns every eligible operation, grouped by caller and in arrival order.
    ///
    /// A map keyed by caller, so the round-robin visits callers in one stable
    /// order however the repository happened to return the rows. Two snapshots
    /// holding the same rows therefore produce the same decisions whatever
    /// order they arrived in.
    fn eligible_by_caller(
        &self,
        observation: &SchedulerObservation,
        now_unix_milliseconds: u64,
    ) -> BTreeMap<Option<String>, Vec<StartDirective>> {
        let mut eligible: Vec<&ScheduledOperation> = observation
            .waiting
            .iter()
            .filter(|operation| operation.is_eligible(now_unix_milliseconds))
            .collect();
        eligible.sort_by(|left, right| {
            left.author_target_identity_digest
                .cmp(&right.author_target_identity_digest)
                .then(left.enqueue_sequence.cmp(&right.enqueue_sequence))
                .then(left.operation_identifier.cmp(&right.operation_identifier))
        });
        let mut queues: BTreeMap<Option<String>, Vec<StartDirective>> = BTreeMap::new();
        for operation in eligible {
            queues.entry(operation.caller_identity.clone()).or_default().push(StartDirective {
                author_target_identity_digest: operation.author_target_identity_digest.clone(),
                operation_identifier: operation.operation_identifier.clone(),
            });
        }
        queues
    }
}
