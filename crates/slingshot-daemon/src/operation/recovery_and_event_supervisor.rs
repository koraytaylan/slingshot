//! Making progress while the daemon is alive, instead of restarting it.
//!
//! Restart is not a retry mechanism. Work that is recoverable has to become
//! unrecoverable or finished while the process that owns it keeps running, so
//! one supervisor per author-target partition holds the schedule, the pauses,
//! and the mapping from what the agent said to what this daemon may conclude.
//!
//! # Nothing here is settled by wall-clock arithmetic
//!
//! Delays are computed from an injected monotonic clock and an injected random
//! sample, and the wall clock appears only as a diagnostic instant that lets a
//! restart reconstruct a remaining wait. A clock that jumped forward makes work
//! due now and one that jumped backwards cannot make a wait longer than it was;
//! neither touches an identity, a state, or a certainty.
//!
//! # Exhaustion is not an outcome
//!
//! Running out of automatic attempts says something about this daemon, not
//! about the remote system. Only a proof that nothing was executed lets
//! exhaustion end an operation; everything else pauses as work a person can
//! resume, because a daemon that terminalized on its own retry budget would be
//! reporting its patience as a remote fact.
//!
//! # The category mapping refuses to guess
//!
//! Every published failure category maps to exactly one disposition, and a
//! category this build does not publish maps to nothing at all. A default would
//! be a guess about a failure whose meaning this build does not know, and the
//! two possible guesses - retry it, or call it rejected - are both wrong in the
//! case that matters.

use std::collections::BTreeMap;

use slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract;
use slingshot_domain::command::catalog::CommandCatalog;

/// The three categories that report an outcome nobody knows.
///
/// Each names a moment where the agent's own record is ambiguous: a replication
/// admitted or not, a package published or not, a mutation applied or not. They
/// stay nonterminal however many attempts are spent on them.
pub const OUTCOME_UNKNOWN_CATEGORIES: &[&str] = &[
    "admission_outcome_unknown",
    "artifact_publication_outcome_unknown",
    "mutation_outcome_unknown",
];

/// Categories where the remote affirmatively failed after taking the request.
///
/// Replication admission is the case: the agent accepted the request and then
/// refused it, so partial effects are possible and calling it a nonexecution
/// would be claiming more than was observed.
pub const REMOTE_FAILURE_CATEGORIES: &[&str] = &["admission_budget_exceeded", "admission_rejected"];

/// The category whose diagnosis outlives the failure it describes.
///
/// A staging area that could not be cleaned up is a maintenance fact about the
/// agent rather than a reason to build the package again, and rebuilding would
/// leave a second staging area beside the first.
pub const STAGING_CLEANUP_CATEGORY: &str = "staging_cleanup_failed";

/// What one failure category means for the operation that met it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CategoryDisposition {
    /// The agent refused it, and provably ran nothing.
    Rejected,
    /// The agent ran it and it failed, possibly with partial effects.
    RemoteFailed,
    /// Nobody knows whether it ran, and more events will not say.
    RecoveryRequired,
    /// It succeeded remotely, and the result can no longer be fetched.
    ResultUnavailable,
    /// The agent held it once and no longer does.
    RecoveryWindowExpired,
}

/// Returns what `category` means, or nothing when this build does not know it.
///
/// # Panics
///
/// Panics only if the published catalog cannot be read, which is a defect in
/// this build rather than a runtime condition.
#[must_use]
pub fn disposition_for(category: &str) -> Option<CategoryDisposition> {
    if OUTCOME_UNKNOWN_CATEGORIES.contains(&category) {
        return Some(CategoryDisposition::RecoveryRequired);
    }
    if REMOTE_FAILURE_CATEGORIES.contains(&category) {
        return Some(CategoryDisposition::RemoteFailed);
    }
    published_categories().contains(&category.to_owned()).then_some(CategoryDisposition::Rejected)
}

/// Returns every failure category this build publishes.
#[must_use]
pub fn published_categories() -> Vec<String> {
    let mut named: Vec<String> = CommandCatalog::published()
        .descriptors()
        .iter()
        .flat_map(|descriptor| descriptor.failure_categories.clone())
        .collect();
    named.sort();
    named.dedup();
    named
}

/// Returns whether `category` carries a diagnosis maintenance must keep.
#[must_use]
pub fn preserves_maintenance_diagnosis(category: &str) -> bool {
    category == STAGING_CLEANUP_CATEGORY
}

/// Returns whether `category` permits building the same artifact again.
#[must_use]
pub fn permits_rebuild(category: &str) -> bool {
    category != STAGING_CLEANUP_CATEGORY
}

/// Why one post-success artifact could not be fetched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactUnavailability {
    /// A fully identified answer saying the artifact is gone, past its grace.
    IdentifiedMissingBeyondGrace,
    /// A fully identified answer saying the retention window closed.
    IdentifiedRetentionExpired,
    /// An answer with no identity on it at all.
    Bare,
    /// An answer this build cannot read.
    Malformed,
    /// An answer identifying another generation, operation, artifact, or slot.
    Mismatched,
}

/// Returns what one artifact unavailability may conclude.
///
/// Only a fully identified answer may end anything. A bare, malformed, or
/// mismatched one says nothing about this daemon's artifact, and terminalizing
/// on it would let a wire response choose a disposition it was never asked for.
#[must_use]
pub fn artifact_disposition(unavailability: ArtifactUnavailability) -> CategoryDisposition {
    match unavailability {
        ArtifactUnavailability::IdentifiedMissingBeyondGrace
        | ArtifactUnavailability::IdentifiedRetentionExpired => {
            CategoryDisposition::ResultUnavailable
        }
        ArtifactUnavailability::Bare
        | ArtifactUnavailability::Malformed
        | ArtifactUnavailability::Mismatched => CategoryDisposition::RecoveryRequired,
    }
}

/// What this daemon knows about whether a command effect happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionCertainty {
    /// It provably did not.
    ConfirmedNotExecuted,
    /// Nobody knows.
    RemoteOutcomeUnknown,
    /// It provably did, and the result is what is missing.
    AuthoritativeRemoteSuccess,
}

/// What running out of automatic attempts produces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exhaustion {
    /// The operation ends, because nothing ran.
    AuthoritativeNonExecution,
    /// The operation waits for a person, because this says nothing about it.
    RecoveryRequired,
}

/// Returns what exhaustion means, given what is known about the effect.
///
/// One certainty may end an operation and two may not. Exhaustion is a fact
/// about this daemon's retry budget, and a daemon that ended work on it would
/// be reporting its own patience as a remote outcome.
#[must_use]
pub fn on_exhaustion(certainty: ExecutionCertainty) -> Exhaustion {
    match certainty {
        ExecutionCertainty::ConfirmedNotExecuted => Exhaustion::AuthoritativeNonExecution,
        ExecutionCertainty::RemoteOutcomeUnknown
        | ExecutionCertainty::AuthoritativeRemoteSuccess => Exhaustion::RecoveryRequired,
    }
}

/// Which kind of work one retry is.
///
/// Named because attempts are counted and paced per category: a stream that
/// keeps dropping must not spend the budget that an ambiguous submission needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RetryCategory {
    /// A submission whose fate is unclear.
    AmbiguousSubmission,
    /// An artifact that has not been fetched yet.
    ArtifactAcquisition,
    /// A filtered stream that dropped.
    EventReconnect,
    /// A result that has not been fetched yet.
    ResultAcquisition,
    /// A snapshot poll while a subscription is degraded.
    SnapshotPoll,
}

/// Returns how many automatic attempts one category is given.
#[must_use]
pub fn automatic_attempt_cap() -> u64 {
    AuthorAgentTransportContract::embedded().limit("maximum_automatic_retry_attempts")
}

/// One wait, as it is written down before it is waited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetrySchedule {
    /// Which attempt this is.
    pub attempt: u64,
    /// Which kind of work is waiting.
    pub category: RetryCategory,
    /// How long to wait, chosen from within this attempt's interval.
    pub chosen_delay_milliseconds: u64,
    /// When the wait ends by the wall clock, for diagnosis and for restarts.
    pub eligible_at_unix_milliseconds: u64,
    /// The largest delay this attempt could have chosen.
    pub jitter_ceiling_milliseconds: u64,
}

/// Returns the wait one attempt produces from one random sample.
///
/// Capped exponential backoff with full jitter, and persisted before it is
/// waited: a wait that existed only in memory would be lost by the restart it
/// is most likely to be interrupted by.
#[must_use]
pub fn schedule(
    category: RetryCategory,
    attempt: u64,
    sample: u64,
    now_unix_milliseconds: u64,
) -> RetrySchedule {
    let ceiling = jitter_ceiling_milliseconds(attempt);
    let chosen = sample % (ceiling + 1);
    RetrySchedule {
        attempt,
        category,
        chosen_delay_milliseconds: chosen,
        eligible_at_unix_milliseconds: now_unix_milliseconds.saturating_add(chosen),
        jitter_ceiling_milliseconds: ceiling,
    }
}

/// Returns the largest delay attempt `attempt` may choose within.
#[must_use]
pub fn jitter_ceiling_milliseconds(attempt: u64) -> u64 {
    let contract = AuthorAgentTransportContract::embedded();
    let initial = contract.limit("retry_base_milliseconds");
    let maximum = contract.limit("retry_jitter_cap_milliseconds");
    let exponent = u32::try_from(attempt.saturating_sub(1)).unwrap_or(u32::MAX);
    initial
        .saturating_mul(DELAY_MULTIPLIER.saturating_pow(exponent.min(MAXIMUM_EXPONENT)))
        .min(maximum)
}

/// The largest exponent worth computing before the cap decides the answer.
const MAXIMUM_EXPONENT: u32 = 32;

/// What each attempt multiplies the previous interval by.
const DELAY_MULTIPLIER: u64 = 2;

/// Returns what remains of a persisted wait after a restart.
///
/// The residual is clamped into the wait that was actually chosen, so a
/// forward jump makes the work due now and a backward jump cannot lengthen it.
/// Either way the answer is bounded by a decision already made, which is why a
/// virtual clock can move scheduling and nothing else.
#[must_use]
pub fn resumed_delay_milliseconds(schedule: &RetrySchedule, now_unix_milliseconds: u64) -> u64 {
    schedule
        .eligible_at_unix_milliseconds
        .saturating_sub(now_unix_milliseconds)
        .min(schedule.chosen_delay_milliseconds)
}

/// One piece of work the supervisor is holding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DueWork {
    /// What it is called at the agent.
    pub agent_operation_identifier: String,
    /// Which kind of work it is.
    pub category: RetryCategory,
    /// When it may next be attempted.
    pub eligible_at_unix_milliseconds: u64,
    /// Whether a person has to release it before it moves.
    pub paused: bool,
}

/// Why a resume was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ResumeRefusal {
    /// The resume names an environment revision this work was not submitted at.
    #[error("this work was submitted under another environment revision")]
    WrongSelectedRevision,
    /// The resume quotes an operation revision that has since moved.
    #[error("this operation is at revision {stored}, and the resume quotes {quoted}")]
    StaleOperationRevision {
        /// What the resume quoted.
        quoted: u64,
        /// What is stored.
        stored: u64,
    },
    /// The resume names a category this work is not paused in.
    #[error("this work is paused in another recovery category")]
    WrongCategory,
}

/// One resume, as a person asked for it.
///
/// Held together because the gates are checked together: a resume that named
/// the right operation under the wrong revision, or quoted a revision that has
/// since moved, is not a resume of this work at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumeRequest {
    /// What the work is called at the agent.
    pub agent_operation_identifier: String,
    /// Which category it is paused in.
    pub category: RetryCategory,
    /// Which operation revision the resume was written against.
    pub quoted_operation_revision: u64,
    /// What the receipt is called.
    pub receipt_identifier: String,
    /// Which environment revision the resume was asked under.
    pub supplied_revision: String,
}

/// What consuming one resume receipt did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResumeOutcome {
    /// It is the first time, so the stored category wakes.
    Woken(RetryCategory),
    /// It has been consumed before, so nothing happens at all.
    Replayed,
}

/// One supervisor, over one author-target partition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryAndEventSupervisor {
    /// Which partition this supervises.
    author_target_identity_digest: String,
    /// Which resume receipts have already been consumed.
    consumed_receipts: Vec<String>,
    /// How many times each category has been served, for fairness.
    served: BTreeMap<RetryCategory, u64>,
    /// Whether new admissions are still accepted.
    shutting_down: bool,
    /// What it is holding.
    work: Vec<DueWork>,
}

impl RecoveryAndEventSupervisor {
    /// Returns a supervisor over `author_target_identity_digest`, holding nothing.
    #[must_use]
    pub fn over(author_target_identity_digest: &str) -> Self {
        Self {
            author_target_identity_digest: author_target_identity_digest.to_owned(),
            consumed_receipts: Vec::new(),
            served: BTreeMap::new(),
            shutting_down: false,
            work: Vec::new(),
        }
    }

    /// Returns which partition this supervises.
    #[must_use]
    pub fn partition(&self) -> &str {
        &self.author_target_identity_digest
    }

    /// Returns everything it is holding.
    #[must_use]
    pub fn work(&self) -> &[DueWork] {
        &self.work
    }

    /// Takes one more piece of work.
    pub fn hold(&mut self, work: DueWork) {
        self.work.push(work);
    }

    /// Pauses one piece of work, as an exhausted automatic policy does.
    ///
    /// Returns whether anything was paused.
    pub fn pause(&mut self, agent_operation_identifier: &str) -> bool {
        let mut paused = false;
        for held in &mut self.work {
            if held.agent_operation_identifier == agent_operation_identifier {
                held.paused = true;
                paused = true;
            }
        }
        paused
    }

    /// Returns what to attempt next, and records that the category was served.
    ///
    /// Due work comes before new admission, and the least-served category comes
    /// before the earliest deadline. A stream that drops constantly would
    /// otherwise starve an ambiguous submission that has been waiting all day,
    /// which is exactly the operation somebody is watching.
    pub fn next_due(&mut self, now_unix_milliseconds: u64) -> Option<DueWork> {
        let served = self.served.clone();
        let chosen = self
            .work
            .iter()
            .filter(|held| {
                !held.paused && held.eligible_at_unix_milliseconds <= now_unix_milliseconds
            })
            .min_by_key(|held| {
                (
                    served.get(&held.category).copied().unwrap_or_default(),
                    held.eligible_at_unix_milliseconds,
                    held.agent_operation_identifier.clone(),
                )
            })
            .cloned()?;
        *self.served.entry(chosen.category).or_default() += 1;
        Some(chosen)
    }

    /// Returns how many times each category has been served.
    #[must_use]
    pub fn served(&self) -> &BTreeMap<RetryCategory, u64> {
        &self.served
    }

    /// Consumes one resume receipt, waking exactly the category it stored.
    ///
    /// # Errors
    ///
    /// Returns [`ResumeRefusal`] naming the first gate the resume fails, none
    /// of which changes anything.
    pub fn resume(
        &mut self,
        request: &ResumeRequest,
        submitted_revision: &str,
        stored_operation_revision: u64,
    ) -> Result<ResumeOutcome, ResumeRefusal> {
        if request.supplied_revision != submitted_revision {
            return Err(ResumeRefusal::WrongSelectedRevision);
        }
        if request.quoted_operation_revision != stored_operation_revision {
            return Err(ResumeRefusal::StaleOperationRevision {
                quoted: request.quoted_operation_revision,
                stored: stored_operation_revision,
            });
        }
        if !self.holds(request) {
            return Err(ResumeRefusal::WrongCategory);
        }
        if self.consumed_receipts.iter().any(|held| held == &request.receipt_identifier) {
            return Ok(ResumeOutcome::Replayed);
        }
        self.consumed_receipts.push(request.receipt_identifier.clone());
        for held in &mut self.work {
            if held.agent_operation_identifier == request.agent_operation_identifier
                && held.category == request.category
            {
                held.paused = false;
            }
        }
        Ok(ResumeOutcome::Woken(request.category))
    }

    /// Returns whether this supervisor holds the work `request` names.
    fn holds(&self, request: &ResumeRequest) -> bool {
        self.work.iter().any(|held| {
            held.agent_operation_identifier == request.agent_operation_identifier
                && held.category == request.category
        })
    }

    /// Stops taking new work and lets go of what is running.
    ///
    /// Returns what was being held. Nothing remote is cancelled: a Sling job
    /// this daemon started is the agent's to finish, and cancelling on the way
    /// out would destroy work whose result somebody will come back for.
    pub fn detach(&mut self) -> Vec<DueWork> {
        self.shutting_down = true;
        std::mem::take(&mut self.work)
    }

    /// Returns whether this supervisor still takes new work.
    #[must_use]
    pub fn accepts_new_work(&self) -> bool {
        !self.shutting_down
    }
}
