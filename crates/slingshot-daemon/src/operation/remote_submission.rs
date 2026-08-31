//! Handing one command to the agent, and deciding what may follow.
//!
//! Everything expensive happens in a fixed order, and the order is the design.
//! What this build has is checked before the network is touched, the durable
//! remote child is written before the request is sent, and the fence is claimed
//! before an effect may exist. A step out of order is a step that either sends
//! something this build cannot account for or records something the agent never
//! heard about.
//!
//! # Drift refuses before anything is spent
//!
//! Before every retry the persisted identity is recomputed from what the build
//! now has and compared. A build whose contracts have moved must not resend a
//! submission derived under the old ones, and it must find that out before it
//! reaches a credential provider or a socket, because both of those are
//! observable elsewhere.
//!
//! # Ambiguity is resolved by asking, never by renaming
//!
//! A request that may or may not have arrived is settled by looking the same
//! submission up under the same names. Deriving a replacement identity to try
//! again would turn one command into two, which is exactly the failure the
//! whole derivation scheme exists to prevent.

use slingshot_agent_connection::command_submission::{Submission, SubmissionOutcome};
use slingshot_storage::operation::remote_submission::FenceFacts;

/// How many physical Sling records prove nothing has started yet.
const NO_PHYSICAL_RECORDS: usize = 0;

/// What the supplied target and revision mean for work already persisted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuppliedIdentity {
    /// The same target and the same revision, so this is the same work.
    Compatible,
    /// The same target under a different revision, which old work predates.
    RevisionChanged,
    /// Another target entirely, so nothing here is about it.
    TargetDisjoint,
}

/// Returns what a supplied target and revision mean for persisted work.
///
/// The target is checked first because it is the coarser partition: work
/// against another author is not old work at all, and calling it a revision
/// change would suggest the two had something to do with each other.
#[must_use]
pub fn classify_supplied(
    persisted_target: &str,
    persisted_revision: &str,
    supplied_target: &str,
    supplied_revision: &str,
) -> SuppliedIdentity {
    if persisted_target != supplied_target {
        return SuppliedIdentity::TargetDisjoint;
    }
    if persisted_revision != supplied_revision {
        return SuppliedIdentity::RevisionChanged;
    }
    SuppliedIdentity::Compatible
}

/// Why a submission may not be sent or resent.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PreflightRefusal {
    /// What this build derives is not what was persisted.
    #[error("this build derives a different submission than the one persisted")]
    DerivationDrifted,
    /// The environment revision has moved under existing work.
    #[error("this operation was submitted under another environment revision")]
    RevisionChanged,
    /// The supplied target names another partition entirely.
    #[error("this operation belongs to another target partition")]
    TargetDisjoint,
    /// The work has passed the point of no return.
    #[error("this work has started, and nothing after that authorizes another effect")]
    AlreadyStarted,
    /// The agent's store was rebuilt, so the persisted names mean nothing.
    #[error(
        "this submission was derived under generation {persisted}, and the agent is on {current}"
    )]
    GenerationChanged {
        /// Which generation the agent is on now.
        current: u64,
        /// Which generation the submission was derived under.
        persisted: u64,
    },
}

/// Requires one persisted submission to be one this build may send again.
///
/// Byte-preserving: nothing here rewrites what was stored, so a build that
/// refuses can be replaced by one that agrees and find the submission exactly
/// as it was left.
///
/// # Errors
///
/// Returns [`PreflightRefusal`] naming the first thing that does not agree.
pub fn require_resendable(
    persisted: &Submission,
    rebuilt: &Submission,
    supplied_target: &str,
    supplied_revision: &str,
    current_generation: u64,
    fence: &FenceFacts,
) -> Result<(), PreflightRefusal> {
    match classify_supplied(
        &persisted.operation.author_target_identity_digest,
        &persisted.operation.selected_environment_revision,
        supplied_target,
        supplied_revision,
    ) {
        SuppliedIdentity::TargetDisjoint => return Err(PreflightRefusal::TargetDisjoint),
        SuppliedIdentity::RevisionChanged => return Err(PreflightRefusal::RevisionChanged),
        SuppliedIdentity::Compatible => {}
    }
    if persisted.submitted_command_digest != rebuilt.submitted_command_digest {
        return Err(PreflightRefusal::DerivationDrifted);
    }
    if persisted.canonical_arguments != rebuilt.canonical_arguments {
        return Err(PreflightRefusal::DerivationDrifted);
    }
    if persisted.operation.agent_event_store_generation != current_generation {
        return Err(PreflightRefusal::GenerationChanged {
            current: current_generation,
            persisted: persisted.operation.agent_event_store_generation,
        });
    }
    if fence.has_started() {
        return Err(PreflightRefusal::AlreadyStarted);
    }
    Ok(())
}

/// What a request whose fate is unclear turned out to be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AmbiguityOutcome {
    /// The agent holds it, carried by exactly these physical records.
    Recorded {
        /// The physical Sling jobs, sorted and distinct.
        physical_sling_job_identifiers: Vec<String>,
    },
    /// Nothing was recorded, and nothing has started, so it may be sent again.
    MayAttemptAgain,
    /// Nothing was recorded, but something may have started, so it may not.
    FailClosed,
}

/// Returns what a lookup after an unclear request settles.
///
/// Several physical records for one logical submission is ordinary and is
/// recorded as the sorted set it is. Zero records permits another attempt only
/// while nothing has started; after the checkpoint, zero records means the
/// lookup did not find what may nonetheless exist, and trying again would risk
/// a second effect.
#[must_use]
pub fn resolve_ambiguity(physical_matches: &[String], fence: &FenceFacts) -> AmbiguityOutcome {
    if physical_matches.len() > NO_PHYSICAL_RECORDS {
        let mut sorted: Vec<String> = physical_matches.to_vec();
        sorted.sort();
        sorted.dedup();
        return AmbiguityOutcome::Recorded { physical_sling_job_identifiers: sorted };
    }
    if fence.permits_another_effect() {
        AmbiguityOutcome::MayAttemptAgain
    } else {
        AmbiguityOutcome::FailClosed
    }
}

/// What the daemon durably records about one handoff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HandoffDisposition {
    /// The agent has it, and this submission is settled remotely.
    Accepted,
    /// The agent had it already, which is the same thing.
    Duplicate,
    /// Provably nothing was recorded, and provably nothing ran.
    NotExecuted,
    /// The window in which the agent would have answered has closed.
    RecoveryWindowExpired,
    /// This identifier already means something else at the agent.
    Conflict,
    /// Nothing is settled and the identical submission may go again.
    RetryAfter {
        /// How long to wait.
        milliseconds: u64,
    },
    /// Nobody knows, and the way out is a lookup rather than another send.
    Unknown,
}

/// Returns what one submission outcome means durably.
///
/// The mapping is deliberately narrow. Everything that is not a proof lands in
/// [`HandoffDisposition::Unknown`], whose only resolution is asking the agent
/// by the names already derived.
#[must_use]
pub fn disposition_of(outcome: &SubmissionOutcome) -> HandoffDisposition {
    match outcome {
        SubmissionOutcome::Accepted { .. } => HandoffDisposition::Accepted,
        SubmissionOutcome::Duplicate { .. } => HandoffDisposition::Duplicate,
        SubmissionOutcome::AuthoritativeNonExecution { .. }
        | SubmissionOutcome::ConfirmedNotExecuted { .. } => HandoffDisposition::NotExecuted,
        SubmissionOutcome::RecoveryWindowExpired => HandoffDisposition::RecoveryWindowExpired,
        SubmissionOutcome::Conflict => HandoffDisposition::Conflict,
        SubmissionOutcome::RetryAfter { milliseconds } => {
            HandoffDisposition::RetryAfter { milliseconds: *milliseconds }
        }
        SubmissionOutcome::SubmissionUnknown { .. } => HandoffDisposition::Unknown,
    }
}

impl HandoffDisposition {
    /// Returns whether this disposition permits sending the same request again.
    ///
    /// Only a bounded wait after a status that settled nothing, and a proof
    /// that nothing was recorded. An unknown outcome does not: its resolution
    /// is a lookup, and resending it is how one command becomes two.
    #[must_use]
    pub fn permits_another_send(&self) -> bool {
        matches!(self, Self::RetryAfter { .. } | Self::NotExecuted)
    }

    /// Returns whether this disposition must be settled by asking the agent.
    #[must_use]
    pub fn requires_lookup(&self) -> bool {
        matches!(self, Self::Unknown)
    }
}
