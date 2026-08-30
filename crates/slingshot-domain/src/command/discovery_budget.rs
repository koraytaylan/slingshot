//! What one discovery job may spend, and when it must stop.
//!
//! Six commands enumerate repository content. Each of them can be pointed at a
//! subtree large enough to run for as long as the repository will let it, so
//! "how much may this cost" cannot be each command's own answer - one command
//! forgetting a counter is one command that never stops.
//!
//! The budget is therefore shared, caller-independent, and read from the limits
//! manifest. A caller can ask for a smaller page; it cannot ask for a larger
//! job.
//!
//! # Cooperative, and honest about it
//!
//! Every check here happens at a boundary the agent reaches on its own:
//! immediately before and after anchor resolution, iterator advancement,
//! property retrieval, reference resolution, and each canonical-output step. At
//! each boundary the order is fixed - cancellation, then elapsed time, then the
//! next charge - so a job that is both cancelled and over time is reported as
//! cancelled, which is the answer the caller asked for.
//!
//! This clock cannot preempt a call that has not returned. A repository call
//! that returns at or after the deadline becomes a duration failure before its
//! value or its error is looked at, and a call that never returns is not this
//! contract's to interrupt. So nothing here claims a wall-clock completion
//! bound, and transport supervision that observes a stalled job must report it
//! as stalled rather than relabel it as a duration-budget failure.
//!
//! # No partial pages
//!
//! A page either completes normally and may carry a continuation token, or the
//! job fails and carries neither matches nor a token. There is no third answer,
//! because a partial page that looked resumable would silently lose whatever
//! the budget cut off.

use serde::Serialize;

use crate::command::result_window::{ResultLimit, ResultOffset};

/// Failure literal every exhausted discovery budget reports.
pub const DISCOVERY_BUDGET_EXCEEDED: &str = "discovery_budget_exceeded";

/// Bytes a Long or a Double property value charges.
///
/// The repository stores both in eight bytes whatever their decimal spelling
/// costs, and charging the spelling would make the same value cost differently
/// depending on how it was written down.
pub const NUMERIC_PROPERTY_BYTES: u64 = 8;

/// Bytes a Boolean property value charges.
pub const BOOLEAN_PROPERTY_BYTES: u64 = 1;

/// Which bound a discovery job ran into.
///
/// These five literals are the whole inventory. Result bytes are absent on
/// purpose: reaching them completes a page rather than failing a job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscoveryBudget {
    /// Candidate nodes inspected.
    CandidateNodes,
    /// Scalar repository values read.
    PropertyValues,
    /// Bytes those values carried.
    PropertyBytes,
    /// Atomic selection tests evaluated.
    CriterionEvaluations,
    /// Time elapsed since the job began.
    ExecutionDuration,
}

/// Reason a discovery job stopped without a page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DiscoveryStop {
    /// The caller cancelled it.
    ///
    /// Distinct from every budget: the job was not too expensive, it was no
    /// longer wanted, and the external job is terminated as cancelled.
    #[error("the discovery job was cancelled")]
    Cancelled,
    /// It ran into one of the five bounds.
    #[error("the discovery job exceeded its {0:?} budget")]
    BudgetExceeded(DiscoveryBudget),
}

impl DiscoveryStop {
    /// Returns the budget this stop names, when a budget caused it.
    #[must_use]
    pub fn budget(self) -> Option<DiscoveryBudget> {
        match self {
            Self::Cancelled => None,
            Self::BudgetExceeded(budget) => Some(budget),
        }
    }
}

/// The exact closed failure an exhausted budget serializes as.
///
/// It carries the failure literal and the budget and nothing else - no partial
/// matches, no token, no count. A caller may retry or narrow the same request;
/// the failure itself asserts no next cursor, because there is none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct DiscoveryBudgetFailure {
    /// Always [`DISCOVERY_BUDGET_EXCEEDED`].
    pub failure: &'static str,
    /// Which of the five bounds was reached.
    pub budget: DiscoveryBudget,
}

impl DiscoveryBudgetFailure {
    /// Returns the closed failure for `budget`.
    #[must_use]
    pub fn new(budget: DiscoveryBudget) -> Self {
        Self { failure: DISCOVERY_BUDGET_EXCEEDED, budget }
    }
}

/// Says whether the caller has asked for this job to stop.
pub trait CancellationSignal {
    /// Returns whether cancellation has been observed.
    fn is_cancelled(&self) -> bool;
}

/// Says how long this job has been running.
///
/// Monotonic and injected, so every boundary - before the deadline, exactly at
/// it, past it - is provable without waiting for one.
pub trait ElapsedMonotonicClock {
    /// Returns milliseconds elapsed since the job began.
    fn elapsed_milliseconds(&self) -> u64;
}

/// What happened to one fully evaluated match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchDisposition {
    /// The remaining initial offset consumed it.
    ///
    /// It cost every charge deciding it required and no result bytes, and it
    /// never becomes a resume key - resuming after a match the caller never saw
    /// would make the offset apply twice.
    SkippedForOffset,
    /// It was admitted to the page.
    Admitted,
    /// The page completed before it, and it is not in the page.
    ///
    /// The next page resumes at it, so nothing is lost.
    PageCompleted,
}

/// Everything one discovery job may spend.
#[derive(Debug)]
pub struct DiscoveryExecutionBudget {
    /// Candidate nodes still available.
    candidate_nodes: u64,
    /// Scalar property values still available.
    property_values: u64,
    /// Property bytes still available.
    property_bytes: u64,
    /// Criterion evaluations still available.
    criterion_evaluations: u64,
    /// Milliseconds at or beyond which the job has run out of time.
    execution_duration_milliseconds: u64,
}

impl DiscoveryExecutionBudget {
    /// Returns one full budget, read from the limits manifest.
    #[must_use]
    pub fn full() -> Self {
        let contract = crate::command::command_identity::CommandContract::embedded();
        Self {
            candidate_nodes: contract.limit("maximum_discovery_candidate_nodes"),
            property_values: contract.limit("maximum_discovery_property_values"),
            property_bytes: contract.limit("maximum_discovery_property_bytes"),
            criterion_evaluations: contract.limit("maximum_discovery_criterion_evaluations"),
            execution_duration_milliseconds: contract
                .limit("maximum_discovery_execution_duration_milliseconds"),
        }
    }

    /// Checks the boundary conditions that precede any charge.
    ///
    /// Cancellation first, then time. Called immediately before and after every
    /// repository call and every canonical-output step.
    ///
    /// # Errors
    ///
    /// Returns [`DiscoveryStop::Cancelled`] when cancellation has been
    /// observed, and [`DiscoveryBudget::ExecutionDuration`] when elapsed time
    /// has reached the deadline. Reaching it exactly is over: the deadline is
    /// the first instant the job no longer has.
    pub fn observe_boundary(
        &self,
        cancellation: &dyn CancellationSignal,
        clock: &dyn ElapsedMonotonicClock,
    ) -> Result<(), DiscoveryStop> {
        if cancellation.is_cancelled() {
            return Err(DiscoveryStop::Cancelled);
        }
        if clock.elapsed_milliseconds() >= self.execution_duration_milliseconds {
            return Err(DiscoveryStop::BudgetExceeded(DiscoveryBudget::ExecutionDuration));
        }
        Ok(())
    }

    /// Reports a repository call that returned at or after the deadline.
    ///
    /// The call's value or error is never interpreted, because a result
    /// produced after the job ran out of time is a result the job was not
    /// entitled to.
    ///
    /// # Errors
    ///
    /// Returns [`DiscoveryBudget::ExecutionDuration`] exactly when the call
    /// returned at or after the deadline.
    pub fn observe_call_return(
        &self,
        clock: &dyn ElapsedMonotonicClock,
    ) -> Result<(), DiscoveryStop> {
        if clock.elapsed_milliseconds() >= self.execution_duration_milliseconds {
            return Err(DiscoveryStop::BudgetExceeded(DiscoveryBudget::ExecutionDuration));
        }
        Ok(())
    }

    /// Charges one inspected candidate node.
    ///
    /// Charged before any of that candidate's properties or criteria, so a
    /// candidate cannot be inspected for free by never being counted.
    ///
    /// # Errors
    ///
    /// Returns [`DiscoveryBudget::CandidateNodes`] when none remain.
    pub fn charge_candidate_node(&mut self) -> Result<(), DiscoveryStop> {
        spend(&mut self.candidate_nodes, 1, DiscoveryBudget::CandidateNodes)
    }

    /// Charges one scalar repository value and the bytes it carried.
    ///
    /// The count is charged before the bytes, so a value that exhausts both is
    /// reported as the count it was.
    ///
    /// # Errors
    ///
    /// Returns [`DiscoveryBudget::PropertyValues`] or
    /// [`DiscoveryBudget::PropertyBytes`] when the respective bound is reached.
    pub fn charge_property_value(&mut self, bytes: u64) -> Result<(), DiscoveryStop> {
        spend(&mut self.property_values, 1, DiscoveryBudget::PropertyValues)?;
        spend(&mut self.property_bytes, bytes, DiscoveryBudget::PropertyBytes)
    }

    /// Charges one atomic selection test.
    ///
    /// # Errors
    ///
    /// Returns [`DiscoveryBudget::CriterionEvaluations`] when none remain.
    pub fn charge_criterion_evaluation(&mut self) -> Result<(), DiscoveryStop> {
        spend(&mut self.criterion_evaluations, 1, DiscoveryBudget::CriterionEvaluations)
    }

    /// Returns how many candidate nodes remain.
    #[must_use]
    pub fn remaining_candidate_nodes(&self) -> u64 {
        self.candidate_nodes
    }

    /// Returns how many scalar property values remain.
    #[must_use]
    pub fn remaining_property_values(&self) -> u64 {
        self.property_values
    }

    /// Returns how many property bytes remain.
    #[must_use]
    pub fn remaining_property_bytes(&self) -> u64 {
        self.property_bytes
    }

    /// Returns how many criterion evaluations remain.
    #[must_use]
    pub fn remaining_criterion_evaluations(&self) -> u64 {
        self.criterion_evaluations
    }

    /// Returns the millisecond at which this job has run out of time.
    #[must_use]
    pub fn execution_deadline_milliseconds(&self) -> u64 {
        self.execution_duration_milliseconds
    }
}

/// Spends `amount` from `remaining`, or reports `budget` exhausted.
///
/// Checked throughout: an amount larger than what remains is exhaustion, never
/// a wrapped counter that would silently refill the budget.
fn spend(remaining: &mut u64, amount: u64, budget: DiscoveryBudget) -> Result<(), DiscoveryStop> {
    match remaining.checked_sub(amount) {
        Some(left) => {
            *remaining = left;
            Ok(())
        }
        None => Err(DiscoveryStop::BudgetExceeded(budget)),
    }
}

/// Bytes one property value of the given repository type charges.
///
/// Binary charges its length without materializing it, because reading a
/// hundred-megabyte binary to find out it is a hundred megabytes is exactly the
/// cost the budget exists to prevent.
#[must_use]
pub fn textual_property_bytes(text: &str) -> u64 {
    u64::try_from(text.len()).unwrap_or(u64::MAX)
}

/// The page one discovery job is building.
///
/// It holds the two things that decide what happens to the next match: how much
/// of the initial offset is left, and how much of the page is left. A
/// continuation resumes with the offset already spent, which is why
/// [`Self::resuming`] takes no offset at all.
#[derive(Debug)]
pub struct DiscoveryPage {
    /// Matches still to be skipped.
    remaining_offset: u64,
    /// Matches this page may still admit.
    remaining_limit: u64,
    /// Result bytes this page may still carry.
    remaining_result_bytes: u64,
    /// Matches admitted so far.
    admitted: u64,
}

impl DiscoveryPage {
    /// Returns the page an initial window begins.
    #[must_use]
    pub fn beginning(offset: ResultOffset, limit: ResultLimit) -> Self {
        Self {
            remaining_offset: offset.count(),
            remaining_limit: limit.count(),
            remaining_result_bytes: result_byte_budget(),
            admitted: 0,
        }
    }

    /// Returns the page a continuation resumes.
    ///
    /// There is no offset: the token resumes strictly after the last emitted
    /// match, so skipping again would skip content the caller has not seen.
    #[must_use]
    pub fn resuming(limit: ResultLimit) -> Self {
        Self {
            remaining_offset: 0,
            remaining_limit: limit.count(),
            remaining_result_bytes: result_byte_budget(),
            admitted: 0,
        }
    }

    /// Disposes of one fully evaluated match that would canonicalize to
    /// `bytes`.
    ///
    /// The order is the contract. A positive remaining offset consumes the
    /// match first, so a skipped match never charges result bytes and never
    /// becomes a resume key. Otherwise the page admits it, unless admitting it
    /// would cross the byte bound - in which case the page completes before it
    /// and the next page begins at it.
    pub fn dispose(&mut self, bytes: u64) -> MatchDisposition {
        if self.remaining_offset > 0 {
            self.remaining_offset -= 1;
            return MatchDisposition::SkippedForOffset;
        }
        if self.remaining_limit == 0 || bytes > self.remaining_result_bytes {
            return MatchDisposition::PageCompleted;
        }
        self.remaining_result_bytes -= bytes;
        self.remaining_limit -= 1;
        self.admitted += 1;
        MatchDisposition::Admitted
    }

    /// Returns whether the page is full and no further repository work is owed.
    ///
    /// Checked immediately after each admission, so a full page wins before
    /// another candidate is charged.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.remaining_limit == 0
    }

    /// Returns how many matches this page has admitted.
    #[must_use]
    pub fn admitted(&self) -> u64 {
        self.admitted
    }

    /// Returns how many matches remain to be skipped.
    #[must_use]
    pub fn remaining_offset(&self) -> u64 {
        self.remaining_offset
    }

    /// Returns how many result bytes remain.
    #[must_use]
    pub fn remaining_result_bytes(&self) -> u64 {
        self.remaining_result_bytes
    }
}

/// Returns the result bytes one page may carry.
fn result_byte_budget() -> u64 {
    crate::command::command_identity::CommandContract::embedded()
        .limit("maximum_discovery_result_bytes")
}
