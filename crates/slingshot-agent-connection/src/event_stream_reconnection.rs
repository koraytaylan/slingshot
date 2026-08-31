//! Getting one lost stream back without inventing anything while it is gone.
//!
//! A dropped stream is an ordinary event, and almost everything about handling
//! it is about restraint. The connection is this daemon's; the work is the
//! agent's. So reconnecting resumes from a position the agent actually issued
//! and this daemon actually committed, over a route built entirely from facts
//! it persisted, and it changes nothing about any job on the way.
//!
//! # What is resumed from
//!
//! The cursor sent on reconnection is the one written durably to the
//! subscription ledger, never the last one seen. A cursor that arrived and was
//! not committed describes events this daemon may not have applied, and
//! resuming past them would skip them silently. The ledger is the subscription's
//! own, not a job's: cursor progress is not conditioned on any local job
//! existing, because a stream can legitimately carry events about work this
//! daemon does not hold.
//!
//! # What the delay is
//!
//! Capped exponential backoff with full jitter, from named constants and an
//! injected sample, so a fleet of daemons that all lost the same author do not
//! return in step. Nothing sleeps here: a schedule is a value, carrying the
//! attempt, the category, the chosen delay, and a diagnostic wall-clock instant
//! that exists so a restart can reconstruct the remaining wait without trusting
//! the wall clock to have behaved.
//!
//! # What is refused
//!
//! A server-offered route, because selecting one author origin means asking
//! nowhere else. A trailer nobody declared, which is protocol loss and carries
//! no cursor of its own. And a second digest for a cursor already committed,
//! which is the one case where reconnecting is not enough: two different
//! contents under one position mean the stream and the ledger disagree, so the
//! cursor stays as it is, the subscription goes Degraded, and streaming does
//! not resume until a full high-water snapshot reset has settled it.

use slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract;

use crate::author_hypertext_transfer_protocol_policy::{
    ResponseHead, ResponseRefusal, retry_delay_milliseconds,
};
use crate::server_sent_event_decoder::EventStreamCursor;

/// The one route a filtered event stream is asked for on.
pub const EVENT_ROUTE: &str = "/libs/slingshot/agent/events";

/// The query member naming which subscription is wanted.
pub const SUBSCRIPTION_QUERY_MEMBER: &str = "daemon_subscription_identifier";

/// The query member naming which incarnation of the store is wanted.
pub const GENERATION_QUERY_MEMBER: &str = "agent_event_store_generation";

/// The header carrying where a reconnection resumes from.
pub const LAST_EVENT_IDENTIFIER_HEADER: &str = "Last-Event-ID";

/// The category every event-stream reconnection is persisted under.
pub const RECONNECT_CATEGORY: &str = "event-reconnect";

/// What each attempt multiplies the previous ceiling by.
pub const DELAY_MULTIPLIER: u64 = 2;

/// The largest exponent worth computing before the cap decides the answer.
const MAXIMUM_EXPONENT: u32 = 32;

/// The first attempt a fresh connection is at.
pub const FIRST_ATTEMPT: u64 = 1;

/// Returns the delay the first reconnection may be scheduled within.
#[must_use]
pub fn initial_delay_milliseconds() -> u64 {
    AuthorAgentTransportContract::embedded().limit("retry_base_milliseconds")
}

/// Returns the largest delay any reconnection may be scheduled within.
#[must_use]
pub fn maximum_delay_milliseconds() -> u64 {
    AuthorAgentTransportContract::embedded().limit("retry_jitter_cap_milliseconds")
}

/// Returns how many times reconnecting is attempted before giving up.
#[must_use]
pub fn maximum_attempts() -> u64 {
    AuthorAgentTransportContract::embedded().limit("maximum_automatic_retry_attempts")
}

/// Why one stream is being reconnected.
///
/// Closed, and persisted with the schedule, so a daemon that comes back knows
/// what it was recovering from rather than only that it was waiting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReconnectCause {
    /// The agent closed the stream tidily.
    CleanClose,
    /// Nothing arrived for longer than the heartbeat timeout.
    HeartbeatTimeout,
    /// The framing failed, or a trailer nobody declared arrived.
    ProtocolLoss,
    /// The attempt was answered with a status that settles nothing.
    RetryableStatus {
        /// Which status it was answered with.
        status: u16,
    },
    /// The connection failed before an answer.
    TransportFailure,
}

/// One reconnection, as it is written down.
///
/// A value rather than a sleep. Persisting the attempt, the category, the
/// chosen delay, and a wall-clock instant lets a daemon that restarts mid-wait
/// reconstruct what remains of it without having to trust that the wall clock
/// went forward by exactly the time that passed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReconnectionSchedule {
    /// Which attempt this is.
    pub attempt: u64,
    /// The category this retry is recorded under.
    pub category: &'static str,
    /// How long to wait, chosen from within this attempt's jitter interval.
    pub chosen_delay_milliseconds: u64,
    /// Why the stream is being reconnected.
    pub cause: ReconnectCause,
    /// When the wait ends, by the wall clock, for diagnosis and for restarts.
    pub eligible_at_unix_milliseconds: u64,
    /// The largest delay this attempt could have chosen.
    pub jitter_ceiling_milliseconds: u64,
}

/// What the subscription's stream is fit for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamHealth {
    /// Streaming may continue or resume.
    Healthy,
    /// The ledger and the stream disagree, and nothing resumes until a reset.
    Degraded,
}

/// What committing one cursor did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommitOutcome {
    /// The ledger now sits at a later position.
    Advanced,
    /// The same position with the same contents, so nothing moved.
    Unchanged,
    /// The same position with different contents, which is an incident.
    Conflicted,
}

/// Why a subscription must be rebuilt rather than resumed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResetReason {
    /// The agent no longer holds the position this daemon resumed from.
    CursorExpired,
    /// The store was rebuilt, so the old positions name nothing.
    GenerationChanged,
    /// One position arrived twice with different contents.
    EqualCursorDigestConflict,
}

/// How a subscription gets back to a position both sides agree on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryRoute {
    /// Take the whole subscription's high-water snapshot and start from it.
    HighWaterSnapshotReset,
}

/// Returns how one reset reason is recovered from.
///
/// One route for all three. A reset reason is by definition a case where the
/// remembered position means nothing, and there is no partial repair of a
/// position that means nothing.
#[must_use]
pub fn reset_route(reason: ResetReason) -> RecoveryRoute {
    match reason {
        ResetReason::CursorExpired
        | ResetReason::GenerationChanged
        | ResetReason::EqualCursorDigestConflict => RecoveryRoute::HighWaterSnapshotReset,
    }
}

/// Why a reconnection cannot be attempted or a connection cannot be kept.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReconnectionRefusal {
    /// The response head is one the shared policy refuses.
    #[error(transparent)]
    Head(#[from] ResponseRefusal),
    /// A trailer section arrived that the head never declared.
    #[error("a trailer nobody declared is protocol loss, and carries no position of its own")]
    UndeclaredTrailer,
    /// The response tried to send the next attempt somewhere else.
    #[error("this stream is asked for at {EVENT_ROUTE}, and a server naming another is refused")]
    ServerRouteOffered {
        /// Where it pointed.
        offered: String,
    },
    /// The query the persisted facts produce is longer than a query may be.
    #[error("one route query holds at most {allowed} bytes, and this holds {actual}")]
    QueryTooLong {
        /// How long one may be.
        allowed: u64,
        /// How long this is.
        actual: usize,
    },
    /// Reconnecting has been tried as often as it is going to be.
    #[error("this stream has been reconnected {allowed} times without success")]
    AttemptsExhausted {
        /// How many times it is tried.
        allowed: u64,
    },
    /// The subscription is degraded and nothing resumes until it is reset.
    #[error("this subscription is degraded, and streaming resumes after a high-water reset")]
    ResetRequired,
}

/// What one subscription has durably applied, and what it was.
///
/// The digest is kept beside the cursor because a position on its own cannot
/// detect the case that matters: the same position arriving twice with
/// different contents. Without it, the second arrival would look like an
/// ordinary no-op.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubscriptionLedger {
    /// The position durably applied, when one has been.
    committed_cursor: Option<EventStreamCursor>,
    /// What was at that position.
    committed_digest: Option<String>,
    /// Which incarnation of the store these positions belong to.
    generation: u64,
}

impl SubscriptionLedger {
    /// Returns an empty ledger for `generation`.
    #[must_use]
    pub fn empty(generation: u64) -> Self {
        Self { committed_cursor: None, committed_digest: None, generation }
    }

    /// Returns which incarnation of the store these positions belong to.
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Returns the position a reconnection resumes from, when there is one.
    ///
    /// Committed only. A cursor that arrived and was not written down describes
    /// events this daemon may not have applied, and resuming past them would
    /// skip them without anybody noticing.
    #[must_use]
    pub fn last_event_identifier(&self) -> Option<&str> {
        self.committed_cursor.as_ref().map(EventStreamCursor::as_text)
    }

    /// Commits one applied position, and says what that did.
    ///
    /// A repeated position with the same contents moves nothing, which is what
    /// a replayed event should do. A repeated position with different contents
    /// moves nothing either, and additionally is an incident: the ledger keeps
    /// what it had, because choosing between two disagreeing accounts of one
    /// position is exactly what this daemon cannot do.
    pub fn commit(&mut self, cursor: &EventStreamCursor, digest: &str) -> CommitOutcome {
        if self.committed_cursor.as_ref() == Some(cursor) {
            return if self.committed_digest.as_deref() == Some(digest) {
                CommitOutcome::Unchanged
            } else {
                CommitOutcome::Conflicted
            };
        }
        self.committed_cursor = Some(cursor.clone());
        self.committed_digest = Some(digest.to_owned());
        CommitOutcome::Advanced
    }

    /// Forgets every position, as a high-water reset does.
    pub fn reset(&mut self, generation: u64) {
        self.committed_cursor = None;
        self.committed_digest = None;
        self.generation = generation;
    }
}

/// One subscription's stream, across however many connections it takes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventStreamReconnection {
    /// Which attempt the next connection is.
    attempt: u64,
    /// What the stream is fit for.
    health: StreamHealth,
    /// What has been durably applied.
    ledger: SubscriptionLedger,
    /// Which reset is outstanding, when one is.
    outstanding_reset: Option<ResetReason>,
    /// Which subscription this is.
    subscription: String,
}

impl EventStreamReconnection {
    /// Returns a subscription that has not connected yet.
    #[must_use]
    pub fn opening(subscription: &str, generation: u64) -> Self {
        Self {
            attempt: FIRST_ATTEMPT,
            health: StreamHealth::Healthy,
            ledger: SubscriptionLedger::empty(generation),
            outstanding_reset: None,
            subscription: subscription.to_owned(),
        }
    }

    /// Returns what this stream is fit for.
    #[must_use]
    pub fn health(&self) -> StreamHealth {
        self.health
    }

    /// Returns which attempt the next connection is.
    #[must_use]
    pub fn attempt(&self) -> u64 {
        self.attempt
    }

    /// Returns what has been durably applied.
    #[must_use]
    pub fn ledger(&self) -> &SubscriptionLedger {
        &self.ledger
    }

    /// Returns the reset that must happen before streaming resumes.
    #[must_use]
    pub fn outstanding_reset(&self) -> Option<ResetReason> {
        self.outstanding_reset
    }

    /// Returns the position the next connection resumes from.
    #[must_use]
    pub fn last_event_identifier(&self) -> Option<&str> {
        self.ledger.last_event_identifier()
    }

    /// Returns the route the next connection is asked for on.
    ///
    /// Built entirely from persisted facts, with the two members in canonical
    /// order and nothing else. A route with a member more or a member fewer is
    /// a different subscription, and a route a server chose is a different
    /// author.
    ///
    /// # Errors
    ///
    /// Returns [`ReconnectionRefusal::QueryTooLong`] or
    /// [`ReconnectionRefusal::ResetRequired`].
    pub fn route(&self) -> Result<String, ReconnectionRefusal> {
        if self.outstanding_reset.is_some() {
            return Err(ReconnectionRefusal::ResetRequired);
        }
        let query = format!(
            "{GENERATION_QUERY_MEMBER}={}&{SUBSCRIPTION_QUERY_MEMBER}={}",
            self.ledger.generation(),
            self.subscription
        );
        let allowed = AuthorAgentTransportContract::embedded().limit("maximum_route_query_bytes");
        if u64::try_from(query.len()).unwrap_or(u64::MAX) > allowed {
            return Err(ReconnectionRefusal::QueryTooLong { allowed, actual: query.len() });
        }
        Ok(format!("{EVENT_ROUTE}?{query}"))
    }

    /// Records one connection that the shared policy accepts.
    ///
    /// Only this resets the backoff. A connection that was refused after being
    /// established is not a connection that worked, and treating it as one
    /// would make a server that answers quickly and wrongly look like a server
    /// that answers.
    ///
    /// # Errors
    ///
    /// Returns [`ReconnectionRefusal::Head`],
    /// [`ReconnectionRefusal::ServerRouteOffered`], or
    /// [`ReconnectionRefusal::ResetRequired`].
    pub fn connected(&mut self, head: &ResponseHead) -> Result<(), ReconnectionRefusal> {
        if self.outstanding_reset.is_some() {
            return Err(ReconnectionRefusal::ResetRequired);
        }
        if let Some(offered) = &head.location {
            return Err(ReconnectionRefusal::ServerRouteOffered { offered: offered.clone() });
        }
        head.require_acceptable()?;
        self.attempt = FIRST_ATTEMPT;
        Ok(())
    }

    /// Returns the schedule for the next attempt, and counts it.
    ///
    /// # Errors
    ///
    /// Returns [`ReconnectionRefusal::AttemptsExhausted`].
    pub fn schedule_next(
        &mut self,
        cause: ReconnectCause,
        sample: u64,
        now_unix_milliseconds: u64,
    ) -> Result<ReconnectionSchedule, ReconnectionRefusal> {
        let allowed = maximum_attempts();
        if self.attempt > allowed {
            return Err(ReconnectionRefusal::AttemptsExhausted { allowed });
        }
        let schedule = schedule_for(self.attempt, cause, sample, now_unix_milliseconds);
        self.attempt += 1;
        Ok(schedule)
    }

    /// Commits one applied position, degrading the stream on a conflict.
    ///
    /// A conflict changes the ledger not at all. What it changes is what this
    /// subscription is allowed to do next, which is nothing until a reset.
    pub fn commit_cursor(&mut self, cursor: &EventStreamCursor, digest: &str) -> CommitOutcome {
        let outcome = self.ledger.commit(cursor, digest);
        if matches!(outcome, CommitOutcome::Conflicted) {
            self.health = StreamHealth::Degraded;
            self.outstanding_reset = Some(ResetReason::EqualCursorDigestConflict);
        }
        outcome
    }

    /// Records that the agent says this position or generation means nothing.
    ///
    /// No cursor advance is invented, and none is retracted. The remembered
    /// position simply stops being usable, and the subscription waits for the
    /// one recovery that does not depend on it.
    pub fn require_reset(&mut self, reason: ResetReason) -> RecoveryRoute {
        self.health = StreamHealth::Degraded;
        self.outstanding_reset = Some(reason);
        reset_route(reason)
    }

    /// Records that a high-water reset has rebuilt this subscription.
    pub fn reset_completed(&mut self, generation: u64) {
        self.attempt = FIRST_ATTEMPT;
        self.health = StreamHealth::Healthy;
        self.ledger.reset(generation);
        self.outstanding_reset = None;
    }
}

/// Returns the largest delay attempt `attempt` may choose within.
///
/// Exponential until the cap, and the cap is what makes an unreachable author
/// cost a bounded amount: without it the interval doubles until a daemon that
/// lost a stream on Monday retries it next year.
#[must_use]
pub fn jitter_ceiling_milliseconds(attempt: u64) -> u64 {
    let exponent = u32::try_from(attempt.saturating_sub(FIRST_ATTEMPT)).unwrap_or(MAXIMUM_EXPONENT);
    initial_delay_milliseconds()
        .saturating_mul(DELAY_MULTIPLIER.saturating_pow(exponent.min(MAXIMUM_EXPONENT)))
        .min(maximum_delay_milliseconds())
}

/// Returns the schedule one attempt produces from one random sample.
///
/// Full jitter: the delay is anywhere from nothing up to the ceiling, rather
/// than the ceiling itself. A fleet that all lost the same author returns
/// spread across the interval instead of arriving together and losing it again.
#[must_use]
pub fn schedule_for(
    attempt: u64,
    cause: ReconnectCause,
    sample: u64,
    now_unix_milliseconds: u64,
) -> ReconnectionSchedule {
    let ceiling = jitter_ceiling_milliseconds(attempt);
    let chosen = sample % (ceiling + 1);
    ReconnectionSchedule {
        attempt,
        category: RECONNECT_CATEGORY,
        chosen_delay_milliseconds: chosen,
        cause,
        eligible_at_unix_milliseconds: now_unix_milliseconds.saturating_add(chosen),
        jitter_ceiling_milliseconds: ceiling,
    }
}

/// Returns the delay a server's `Retry-After` produces, bounded by the cap.
///
/// Honoured, because a server that says how long it needs usually knows. Capped,
/// because without a cap the worst a server can do to a stream is unbounded:
/// asking for a year would park the subscription for a year.
#[must_use]
pub fn bounded_retry_after_milliseconds(requested: u64) -> u64 {
    retry_delay_milliseconds(Some(requested))
}

/// Returns what remains of a persisted wait after a restart.
///
/// Reconstructed by clamping the wall-clock residual into the wait that was
/// actually chosen. A clock that jumped forward makes the wait due now; one
/// that jumped backwards cannot make it longer than it ever was. Either way the
/// answer is bounded by a decision this daemon already made, so wall-clock
/// movement cannot lengthen or shorten a wait beyond it.
#[must_use]
pub fn resumed_delay_milliseconds(
    schedule: &ReconnectionSchedule,
    now_unix_milliseconds: u64,
) -> u64 {
    schedule
        .eligible_at_unix_milliseconds
        .saturating_sub(now_unix_milliseconds)
        .min(schedule.chosen_delay_milliseconds)
}

/// Returns what a trailer section nobody declared means for a stream.
#[must_use]
pub fn undeclared_trailer() -> ReconnectionRefusal {
    ReconnectionRefusal::UndeclaredTrailer
}
