//! Running one reconciliation pass over what this daemon persisted.
//!
//! The decision about what an answer means is inward, in the connection crate,
//! and pure. What lives here is the part that needs the daemon's own record:
//! how long the submission has been waiting, which physical Sling jobs were
//! written down before the agent's store was rebuilt, and what to conclude when
//! the answer is that there is no such operation.
//!
//! # A generation change is asked about, never assumed
//!
//! When the store has been rebuilt, the logical identifier names nothing, so
//! the pass asks after every physical job it persisted instead. The distinction
//! it preserves is between an agent that answered "no such job" and an agent
//! that did not answer: the first is evidence and the second is its absence,
//! and collapsing them would turn an unreachable agent into a lost operation.

use slingshot_agent_connection::job_snapshot_reconciliation::{
    Convergence, GenerationLossCertainty, JobSnapshot, LookupAnswer, ReconciliationOutcome,
    ReconciliationRefusal, SnapshotExpectation, TerminalSettlement,
    certainty_after_generation_change, physical_job_routes, reconcile_and_settle,
};
use slingshot_domain::remote_job::RemoteJobObservation;

/// What this daemon persisted about one submission before it asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistedSubmission {
    /// What is known about the job running it.
    pub observation: RemoteJobObservation,
    /// The physical Sling jobs written down for it.
    pub physical_sling_job_identifiers: Vec<String>,
    /// When the request that submitted it started.
    pub submitted_at_unix_milliseconds: u64,
}

/// What one pass over one submission concluded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PassOutcome {
    /// The logical lookup answered, and this is what it meant.
    Logical(Box<ReconciliationOutcome>),
    /// The store was rebuilt, and this is what the physical jobs left known.
    AfterGenerationChange(Box<GenerationLossCertainty>),
}

/// Returns what one ordinary reconciliation pass concludes.
///
/// # Errors
///
/// Returns [`ReconciliationRefusal`] naming the first thing that does not
/// agree, all of which leave every persisted fact exactly as it was.
pub fn pass(
    expectation: &SnapshotExpectation,
    persisted: &PersistedSubmission,
    answer: &LookupAnswer,
    now_unix_milliseconds: u64,
    settlement: &dyn TerminalSettlement,
) -> Result<PassOutcome, ReconciliationRefusal> {
    let outcome = reconcile_and_settle(
        expectation,
        persisted.observation,
        answer,
        persisted.submitted_at_unix_milliseconds,
        now_unix_milliseconds,
        settlement,
    )?;
    Ok(PassOutcome::Logical(Box::new(outcome)))
}

/// Returns the routes a generation-loss recovery asks, in canonical order.
///
/// # Errors
///
/// Returns [`ReconciliationRefusal`] when the persisted jobs cannot all be
/// asked about within the bounds one recovery is held to.
pub fn recovery_routes(
    persisted: &PersistedSubmission,
) -> Result<Vec<String>, ReconciliationRefusal> {
    physical_job_routes(&persisted.physical_sling_job_identifiers)
}

/// Returns what a rebuilt store leaves known about one submission.
///
/// Called only after every route [`recovery_routes`] produced has been asked,
/// because the answer turns on how many of them said "no such job" as against
/// how many said nothing at all.
#[must_use]
pub fn certainty_after_rebuild(
    persisted: &PersistedSubmission,
    recovered: Option<JobSnapshot>,
    answered_missing: usize,
) -> PassOutcome {
    PassOutcome::AfterGenerationChange(Box::new(certainty_after_generation_change(
        &persisted.physical_sling_job_identifiers,
        recovered,
        answered_missing,
    )))
}

/// Returns whether one pass outcome leaves the persisted state untouched.
///
/// True for every outcome but a settled ending. A caller that kept its held
/// state whenever this says so is correct, which is what makes a declining
/// settlement safe rather than merely survivable.
#[must_use]
pub fn leaves_state_untouched(outcome: &PassOutcome) -> bool {
    match outcome {
        PassOutcome::Logical(logical) => !matches!(**logical, ReconciliationOutcome::Settled(_)),
        PassOutcome::AfterGenerationChange(_) => true,
    }
}

/// Returns whether one convergence asks for another look rather than settling.
#[must_use]
pub fn asks_again(convergence: &Convergence) -> bool {
    matches!(convergence, Convergence::GraceRequired { .. })
}
