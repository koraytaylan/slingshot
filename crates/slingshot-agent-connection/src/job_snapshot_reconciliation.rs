//! Asking the agent what is true, because the stream is not a record.
//!
//! Events can be missed, replayed, or arrive after the thing they describe has
//! moved on. A daemon that treated the last event it saw as the truth would be
//! treating its own connection quality as a fact about somebody else's system,
//! so after a reconnection or a restart it asks instead of assuming.
//!
//! # An answer is believed only if it is about this submission
//!
//! Every answer echoes the generation, the target, the subscription, the
//! revision, the contracts, and the submitted digest, and all of them are
//! checked before anything about the job is read. The case this exists for is
//! subtle: a result produced by the same command with different arguments is
//! validly shaped, correctly sequenced, and completely wrong, and only the
//! digest distinguishes it.
//!
//! # Reconciliation never rolls backwards
//!
//! A snapshot older than what is already applied is old news, not a correction.
//! Rolling state or sequence back to it would undo events this daemon has
//! already acted on, so a stale snapshot converges to nothing at all.
//!
//! # Ending is somebody else's transaction
//!
//! Terminal data is handed to an injected settlement boundary rather than
//! persisted here. The bounded conversion, the result validation, and the one
//! atomic snapshot-state-result transaction belong to the task that owns them;
//! what this module owns is the decision that the answer deserves to be
//! settled, and the guarantee that a refusal leaves every fact where it was.

use slingshot_agent_protocol::identity::DocumentProvenance;
use slingshot_agent_protocol::job_contract::JobEventKind;
use slingshot_agent_protocol::wire_contract::{ExpectedProvenance, WireRefusal};
use slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract;
use slingshot_domain::remote_job::{
    AgentJobState, JobEventSequence, RemoteJobFailure, RemoteJobObservation,
};

/// The one route a logical operation is looked up on.
pub const LOOKUP_ROUTE: &str = "/libs/slingshot/agent/operations";

/// The one route a physical Sling job is looked up on.
pub const PHYSICAL_JOB_ROUTE: &str = "/libs/slingshot/agent/jobs";

/// The one route a subscription's high-water position is captured on.
pub const HIGH_WATER_ROUTE: &str = "/libs/slingshot/agent/subscriptions/high-water";

/// The query member naming which logical operation is wanted.
pub const OPERATION_QUERY_MEMBER: &str = "agent_operation_identifier";

/// The query member naming which physical Sling job is wanted.
pub const SLING_JOB_QUERY_MEMBER: &str = "sling_job_identifier";

/// Characters a query value keeps as itself.
///
/// Everything else is percent-encoded, exactly once. A separator that survived
/// into a value would let a Sling job identifier choose which route it went to,
/// and one that was encoded twice would ask about a job nobody has.
const UNRESERVED: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";

/// What every answer must say about which submission it is about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotEcho {
    /// Which incarnation of the store it came from.
    pub agent_event_store_generation: u64,
    /// Which operation it is about.
    pub agent_operation_identifier: String,
    /// Which partition it belongs to.
    pub author_target_identity_digest: String,
    /// Which subscription carries its events.
    pub daemon_subscription_identifier: String,
    /// Which contracts it was produced under.
    pub provenance: DocumentProvenance,
    /// Which environment revision it was submitted under.
    pub selected_environment_revision: String,
    /// Which submission it is.
    pub submitted_command_digest: String,
}

/// What the agent says is true about one job right now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobSnapshot {
    /// How many physical attempts have carried it.
    pub attempt: u64,
    /// What it says about which submission it is about.
    pub echo: SnapshotEcho,
    /// How long the agent promises to keep the results.
    pub granted_retention_milliseconds: u64,
    /// What it is.
    pub kind: JobEventKind,
    /// The physical Sling jobs the agent knows are carrying it.
    pub physical_sling_job_identifiers: Vec<String>,
    /// How far the agent says it has got.
    pub progress: u64,
    /// Which of that job's own events this account covers.
    pub sequence: JobEventSequence,
}

impl JobSnapshot {
    /// Returns the durable state this snapshot describes.
    #[must_use]
    pub fn described_state(&self) -> AgentJobState {
        match self.kind {
            JobEventKind::Accepted => AgentJobState::Queued,
            JobEventKind::Started | JobEventKind::Progress => AgentJobState::Running,
            JobEventKind::Succeeded => AgentJobState::Succeeded,
            JobEventKind::Failed => AgentJobState::Failed,
        }
    }
}

/// What a lookup said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LookupAnswer {
    /// The agent holds it, and says this about it.
    Found(Box<JobSnapshot>),
    /// The agent held it once, and no longer does.
    Retired(Box<SnapshotEcho>),
    /// The agent has never heard of it, or will not say.
    Missing,
}

/// What this build knows about the submission it is asking after.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotExpectation {
    /// Which incarnation of the store it was submitted under.
    pub agent_event_store_generation: u64,
    /// What it is called at the agent.
    pub agent_operation_identifier: String,
    /// Which partition it belongs to.
    pub author_target_identity_digest: String,
    /// Which subscription carries its events.
    pub daemon_subscription_identifier: String,
    /// Which contracts this build has.
    pub expected_provenance: ExpectedProvenance,
    /// Which environment revision it was submitted under.
    pub selected_environment_revision: String,
    /// Which submission it is.
    pub submitted_command_digest: String,
}

impl SnapshotExpectation {
    /// Requires one answer to be about this submission and no other.
    ///
    /// Ordered from the coarsest binding to the finest, so a reader learns the
    /// most fundamental thing that is wrong rather than whichever the code
    /// happened to check first.
    ///
    /// # Errors
    ///
    /// Returns [`ReconciliationRefusal`] naming the first thing that differs.
    pub fn require_echoed(&self, echo: &SnapshotEcho) -> Result<(), ReconciliationRefusal> {
        if echo.author_target_identity_digest != self.author_target_identity_digest {
            return Err(ReconciliationRefusal::AnotherTarget);
        }
        if echo.agent_event_store_generation != self.agent_event_store_generation {
            return Err(ReconciliationRefusal::AnotherGeneration {
                expected: self.agent_event_store_generation,
                named: echo.agent_event_store_generation,
            });
        }
        if echo.agent_operation_identifier != self.agent_operation_identifier {
            return Err(ReconciliationRefusal::AnotherOperation);
        }
        if echo.daemon_subscription_identifier != self.daemon_subscription_identifier {
            return Err(ReconciliationRefusal::AnotherSubscription);
        }
        if echo.selected_environment_revision != self.selected_environment_revision {
            return Err(ReconciliationRefusal::AnotherRevision);
        }
        self.expected_provenance.require_matching(&echo.provenance)?;
        if echo.submitted_command_digest != self.submitted_command_digest {
            return Err(ReconciliationRefusal::AnotherSubmission);
        }
        Ok(())
    }
}

/// Why one answer cannot be reconciled against what is held.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReconciliationRefusal {
    /// The answer is about another partition.
    #[error("this answer names another target partition")]
    AnotherTarget,
    /// The answer is about another incarnation of the store.
    #[error("this daemon asked about generation {expected}, and this answer names {named}")]
    AnotherGeneration {
        /// Which generation was asked about.
        expected: u64,
        /// Which generation the answer names.
        named: u64,
    },
    /// The answer is about another operation.
    #[error("this answer names another operation")]
    AnotherOperation,
    /// The answer is about another subscription.
    #[error("this answer names another subscription")]
    AnotherSubscription,
    /// The answer was produced under another environment revision.
    #[error("this answer names another environment revision")]
    AnotherRevision,
    /// The answer is about another submission of the same command.
    #[error("this answer ends a submission this daemon did not make")]
    AnotherSubmission,
    /// The answer names contracts this build does not have.
    #[error(transparent)]
    Provenance(#[from] WireRefusal),
    /// The answer describes a transition the domain does not allow.
    #[error(transparent)]
    Job(#[from] RemoteJobFailure),
    /// The retention the answer grants is already spent.
    #[error("this answer promises results that are already gone")]
    RetentionExpired,
    /// The job identifier cannot be put in a query at all.
    #[error("a Sling job identifier holds at most {allowed} bytes, and this holds {actual}")]
    IdentifierTooLong {
        /// How long one may be.
        allowed: u64,
        /// How long this is.
        actual: usize,
    },
    /// More physical jobs were named than one recovery may chase.
    #[error("one recovery looks up at most {allowed} physical jobs, and this names {actual}")]
    TooManyPhysicalJobs {
        /// How many it may chase.
        allowed: u64,
        /// How many were named.
        actual: usize,
    },
}

/// What reconciling one answer concluded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Convergence {
    /// The agent agrees with what is already held.
    Unchanged,
    /// The agent is ahead, and this is what is now true.
    Advanced(Box<RemoteJobObservation>),
    /// The answer describes a moment already passed, so nothing moves.
    StaleSnapshot,
    /// The agent says it ended, and the ending is ready to be settled.
    ReadyToSettle(Box<RemoteJobObservation>),
    /// The agent held it once and no longer does.
    RecoveryWindowExpired,
    /// The agent has not heard of it yet, and it is too soon to conclude.
    GraceRequired {
        /// When asking again stops being the answer.
        until_unix_milliseconds: u64,
    },
    /// Nobody knows, and nothing about the answer makes it knowable.
    Indeterminate,
}

/// Returns how long a missing operation is given before it means something.
///
/// A submission that has just been accepted may not be visible yet, and
/// concluding anything from that would turn ordinary propagation delay into a
/// lost operation.
#[must_use]
pub fn missing_grace_milliseconds() -> u64 {
    AuthorAgentTransportContract::embedded().limit("missing_operation_grace_milliseconds")
}

/// Returns what one answer means for what is held, changing nothing.
///
/// Pure. Whether to keep the answer, and how, is the caller's decision, and a
/// function that had already written it down would have made that decision.
///
/// # Errors
///
/// Returns [`ReconciliationRefusal`] naming the first thing that does not
/// agree, all of which leave the held state exactly as it was.
pub fn reconcile(
    expectation: &SnapshotExpectation,
    held: RemoteJobObservation,
    answer: &LookupAnswer,
    submitted_at_unix_milliseconds: u64,
    now_unix_milliseconds: u64,
) -> Result<Convergence, ReconciliationRefusal> {
    match answer {
        LookupAnswer::Missing => {
            Ok(missing_convergence(submitted_at_unix_milliseconds, now_unix_milliseconds))
        }
        LookupAnswer::Retired(echo) => {
            expectation.require_echoed(echo)?;
            Ok(Convergence::RecoveryWindowExpired)
        }
        LookupAnswer::Found(snapshot) => {
            expectation.require_echoed(&snapshot.echo)?;
            converge(held, snapshot, submitted_at_unix_milliseconds, now_unix_milliseconds)
        }
    }
}

/// Returns what a missing answer means, given how long it has been missing.
fn missing_convergence(
    submitted_at_unix_milliseconds: u64,
    now_unix_milliseconds: u64,
) -> Convergence {
    let until = submitted_at_unix_milliseconds.saturating_add(missing_grace_milliseconds());
    if now_unix_milliseconds < until {
        Convergence::GraceRequired { until_unix_milliseconds: until }
    } else {
        Convergence::Indeterminate
    }
}

/// Returns what one believed snapshot means for what is held.
fn converge(
    held: RemoteJobObservation,
    snapshot: &JobSnapshot,
    submitted_at_unix_milliseconds: u64,
    now_unix_milliseconds: u64,
) -> Result<Convergence, ReconciliationRefusal> {
    if snapshot.sequence < held.applied_sequence {
        return Ok(Convergence::StaleSnapshot);
    }
    let state = snapshot.described_state();
    if snapshot.sequence == held.applied_sequence
        && state == held.state
        && snapshot.attempt == held.attempt
        && snapshot.progress == held.progress
    {
        return Ok(Convergence::Unchanged);
    }
    let advanced = held.advanced(state, snapshot.sequence, snapshot.attempt, snapshot.progress)?;
    if !state.is_terminal() {
        return Ok(Convergence::Advanced(Box::new(advanced)));
    }
    let elapsed = now_unix_milliseconds.saturating_sub(submitted_at_unix_milliseconds);
    if elapsed >= snapshot.granted_retention_milliseconds {
        return Err(ReconciliationRefusal::RetentionExpired);
    }
    Ok(Convergence::ReadyToSettle(Box::new(advanced)))
}

/// Returns `value` with every reserved character percent-encoded, once.
#[must_use]
pub fn encoded_once(value: &str) -> String {
    let mut encoded = String::new();
    for octet in value.bytes() {
        if UNRESERVED.as_bytes().contains(&octet) {
            encoded.push(char::from(octet));
        } else {
            encoded.push_str(&format!("%{octet:02X}"));
        }
    }
    encoded
}

/// Returns the route one logical operation is looked up on.
///
/// # Errors
///
/// Returns [`ReconciliationRefusal::IdentifierTooLong`].
pub fn lookup_route(agent_operation_identifier: &str) -> Result<String, ReconciliationRefusal> {
    let allowed =
        AuthorAgentTransportContract::embedded().limit("maximum_agent_operation_identifier_bytes");
    require_within(agent_operation_identifier, allowed)?;
    Ok(format!(
        "{LOOKUP_ROUTE}?{OPERATION_QUERY_MEMBER}={}",
        encoded_once(agent_operation_identifier)
    ))
}

/// Returns the routes one generation-loss recovery asks, in canonical order.
///
/// Sorted and distinct, so two recoveries of the same submission ask the same
/// questions in the same order, and bounded, so a submission the agent says was
/// carried by a great many physical jobs cannot turn one recovery into an
/// unbounded number of requests.
///
/// # Errors
///
/// Returns [`ReconciliationRefusal::TooManyPhysicalJobs`] or
/// [`ReconciliationRefusal::IdentifierTooLong`].
pub fn physical_job_routes(
    sling_job_identifiers: &[String],
) -> Result<Vec<String>, ReconciliationRefusal> {
    let contract = AuthorAgentTransportContract::embedded();
    let allowed_matches = contract.limit("maximum_physical_sling_job_matches");
    let allowed_bytes = contract.limit("maximum_sling_job_identifier_bytes");
    let mut ordered: Vec<&String> = sling_job_identifiers.iter().collect();
    ordered.sort();
    ordered.dedup();
    if u64::try_from(ordered.len()).unwrap_or(u64::MAX) > allowed_matches {
        return Err(ReconciliationRefusal::TooManyPhysicalJobs {
            allowed: allowed_matches,
            actual: ordered.len(),
        });
    }
    ordered
        .into_iter()
        .map(|identifier| {
            require_within(identifier, allowed_bytes)?;
            Ok(format!(
                "{PHYSICAL_JOB_ROUTE}?{SLING_JOB_QUERY_MEMBER}={}",
                encoded_once(identifier)
            ))
        })
        .collect()
}

/// Returns the route one subscription's high-water position is captured on.
///
/// The same two members the event route carries, in the same canonical order,
/// because the reset is about exactly the stream the events came from.
#[must_use]
pub fn high_water_route(
    daemon_subscription_identifier: &str,
    agent_event_store_generation: u64,
) -> String {
    format!(
        "{HIGH_WATER_ROUTE}?agent_event_store_generation={agent_event_store_generation}\
         &daemon_subscription_identifier={}",
        encoded_once(daemon_subscription_identifier)
    )
}

/// Requires one query value to fit its bound.
fn require_within(value: &str, allowed: u64) -> Result<(), ReconciliationRefusal> {
    if u64::try_from(value.len()).unwrap_or(u64::MAX) > allowed {
        return Err(ReconciliationRefusal::IdentifierTooLong { allowed, actual: value.len() });
    }
    Ok(())
}

/// What a generation change leaves known about one submission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GenerationLossCertainty {
    /// A physical job answered, so the work is found again.
    Recovered(Box<JobSnapshot>),
    /// Every physical job answered, and every one said it has no such job.
    KnownMissing,
    /// Nothing answered either way, so nothing is known.
    EvidenceFreeAmbiguous,
    /// There were no physical jobs to ask about at all.
    RemoteStateLost,
}

/// Returns what a generation change leaves known, given what the jobs said.
///
/// Three outcomes and a fourth for having nothing to ask. The distinction that
/// matters is between an agent that answered "no such job" and an agent that
/// did not answer: the first is evidence and the second is its absence, and
/// collapsing them would turn an unreachable agent into a lost operation.
#[must_use]
pub fn certainty_after_generation_change(
    persisted_physical_jobs: &[String],
    recovered: Option<JobSnapshot>,
    answered_missing: usize,
) -> GenerationLossCertainty {
    if let Some(snapshot) = recovered {
        return GenerationLossCertainty::Recovered(Box::new(snapshot));
    }
    if persisted_physical_jobs.is_empty() {
        return GenerationLossCertainty::RemoteStateLost;
    }
    if answered_missing == persisted_physical_jobs.len() {
        return GenerationLossCertainty::KnownMissing;
    }
    GenerationLossCertainty::EvidenceFreeAmbiguous
}

/// The facts a settlement is offered.
///
/// Everything it needs to write one ending, and nothing it does not. The
/// physical job identifiers travel with it because the ending is about work
/// that several physical records may have carried, and a settlement that
/// learned them separately could learn a different set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalFacts {
    /// What the submission is called at the agent.
    pub agent_operation_identifier: String,
    /// Which partition it belongs to.
    pub author_target_identity_digest: String,
    /// What is now true about it.
    pub observation: RemoteJobObservation,
    /// The physical Sling jobs the agent knows carried it.
    pub physical_sling_job_identifiers: Vec<String>,
    /// How long the results survive, counted from the request that made them.
    pub remaining_retention_milliseconds: u64,
    /// Which submission it ends.
    pub submitted_command_digest: String,
}

/// Why a settlement declined to write one ending.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SettlementRefusal {
    /// The settlement will not write this ending, for a reason it names.
    #[error("the settlement declined: {reason}")]
    Declined {
        /// What it said.
        reason: String,
    },
}

/// Whoever owns the transaction that writes an ending down.
pub trait TerminalSettlement {
    /// Writes one ending, atomically, or declines to.
    ///
    /// # Errors
    ///
    /// Returns [`SettlementRefusal`] when it will not, which must leave every
    /// snapshot, job, and result fact exactly as it was.
    fn settle(&self, facts: &TerminalFacts) -> Result<(), SettlementRefusal>;
}

/// What reconciling one answer and offering it to a settlement produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReconciliationOutcome {
    /// The answer did not end anything, and this is what it did.
    Converged(Box<Convergence>),
    /// The ending was written down, and this is what is now true.
    Settled(Box<RemoteJobObservation>),
    /// The ending was offered and declined, so nothing changed.
    SettlementDeclined(Box<SettlementRefusal>),
}

/// Returns what one answer means, settling it when it ends something.
///
/// The settlement is offered the ending only after every echo has been checked,
/// so a refusal by the settlement and a refusal of the answer are never
/// confused: the first means the transaction would not commit, and the second
/// means the answer was never about this submission.
///
/// # Errors
///
/// Returns [`ReconciliationRefusal`] naming the first thing that does not
/// agree, all of which leave the held state exactly as it was.
pub fn reconcile_and_settle(
    expectation: &SnapshotExpectation,
    held: RemoteJobObservation,
    answer: &LookupAnswer,
    submitted_at_unix_milliseconds: u64,
    now_unix_milliseconds: u64,
    settlement: &dyn TerminalSettlement,
) -> Result<ReconciliationOutcome, ReconciliationRefusal> {
    let convergence = reconcile(
        expectation,
        held,
        answer,
        submitted_at_unix_milliseconds,
        now_unix_milliseconds,
    )?;
    let Convergence::ReadyToSettle(ending) = &convergence else {
        return Ok(ReconciliationOutcome::Converged(Box::new(convergence)));
    };
    let LookupAnswer::Found(snapshot) = answer else {
        return Ok(ReconciliationOutcome::Converged(Box::new(convergence)));
    };
    let facts = TerminalFacts {
        agent_operation_identifier: expectation.agent_operation_identifier.clone(),
        author_target_identity_digest: expectation.author_target_identity_digest.clone(),
        observation: **ending,
        physical_sling_job_identifiers: snapshot.physical_sling_job_identifiers.clone(),
        remaining_retention_milliseconds: snapshot
            .granted_retention_milliseconds
            .saturating_sub(now_unix_milliseconds.saturating_sub(submitted_at_unix_milliseconds)),
        submitted_command_digest: expectation.submitted_command_digest.clone(),
    };
    match settlement.settle(&facts) {
        Ok(()) => Ok(ReconciliationOutcome::Settled(Box::new(**ending))),
        Err(refusal) => Ok(ReconciliationOutcome::SettlementDeclined(Box::new(refusal))),
    }
}
