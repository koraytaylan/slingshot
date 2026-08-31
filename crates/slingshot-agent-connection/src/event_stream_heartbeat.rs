//! Deciding when a quiet stream has stopped being a stream.
//!
//! A live event stream is mostly silence. A job that runs for an hour and says
//! nothing is working, and a connection that died an hour ago also says
//! nothing, so silence alone distinguishes them not at all. The agent therefore
//! sends a comment periodically, and the only question this module answers is
//! how long a gap is too long.
//!
//! What it deliberately does not answer is anything about the job. A timeout
//! here says one connection went quiet; it says nothing about whether remote
//! work is running, finished, or failed, and a daemon that concluded a job had
//! failed because its own connection dropped would be recording its network as
//! a fact about somebody else's system. So the whole vocabulary is two
//! connection states, and the only thing a timeout asks for is another
//! connection.
//!
//! Time arrives from outside. A clock that goes backwards is a defect in the
//! caller rather than an event to absorb, and reported as one, because
//! subtracting a larger instant from a smaller would either underflow or
//! silently invent liveness that never happened.

use slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract;

use crate::author_hypertext_transfer_protocol_policy::ExchangeDeadlines;
use crate::server_sent_event_decoder::StreamItem;

/// How long a stream may say nothing before it is presumed gone.
#[must_use]
pub fn heartbeat_timeout_milliseconds() -> u64 {
    AuthorAgentTransportContract::embedded().formula("heartbeat_timeout_milliseconds")
}

/// How often the agent is expected to say something.
#[must_use]
pub fn heartbeat_interval_milliseconds() -> u64 {
    AuthorAgentTransportContract::embedded().limit("heartbeat_interval_milliseconds")
}

/// What one connection is, as far as liveness goes.
///
/// Two states and no third, because there is no third thing this module knows.
/// Any richer vocabulary here would be a place for connection quality to leak
/// into what a daemon believes about remote work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    /// Something arrived recently enough.
    Healthy,
    /// Nothing has arrived for longer than the timeout.
    TimedOut,
}

impl ConnectionState {
    /// Returns whether this state asks for another connection.
    ///
    /// The whole consequence of a timeout. It does not retract an event, mark a
    /// job, or settle anything about remote work.
    #[must_use]
    pub fn requires_reconnection(self) -> bool {
        matches!(self, Self::TimedOut)
    }
}

/// A defect in the caller, surfaced rather than absorbed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum HeartbeatFault {
    /// The injected clock went backwards.
    #[error("a monotonic clock does not go backwards, and this went from {last} to {named}")]
    ClockRegressed {
        /// The latest instant this heartbeat had seen.
        last: u64,
        /// The instant it was handed.
        named: u64,
    },
}

/// One stream's liveness, measured against an injected monotonic clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventStreamHeartbeat {
    /// When something last arrived.
    last_activity_milliseconds: u64,
    /// How long a gap may be.
    timeout_milliseconds: u64,
}

impl EventStreamHeartbeat {
    /// Returns the deadlines attaching to a stream is held to.
    ///
    /// Connecting and waiting for the head. Nothing bounds the body: a live
    /// stream is supposed to stay open, and a total deadline on it would make
    /// every long-running job into a failure at the same moment.
    #[must_use]
    pub fn attachment_deadlines() -> ExchangeDeadlines {
        ExchangeDeadlines::embedded()
    }

    /// Returns a heartbeat that has just seen a stream attach.
    #[must_use]
    pub fn attached_at(now_milliseconds: u64) -> Self {
        Self {
            last_activity_milliseconds: now_milliseconds,
            timeout_milliseconds: heartbeat_timeout_milliseconds(),
        }
    }

    /// Returns a heartbeat held to `timeout_milliseconds` instead.
    #[must_use]
    pub fn attached_with_timeout(now_milliseconds: u64, timeout_milliseconds: u64) -> Self {
        Self { last_activity_milliseconds: now_milliseconds, timeout_milliseconds }
    }

    /// Returns how long a gap this heartbeat allows.
    #[must_use]
    pub fn timeout_milliseconds(&self) -> u64 {
        self.timeout_milliseconds
    }

    /// Returns when something last arrived.
    #[must_use]
    pub fn last_activity_milliseconds(&self) -> u64 {
        self.last_activity_milliseconds
    }

    /// Records that one complete stream item arrived, and returns the state.
    ///
    /// A comment and an event refresh liveness identically. The distinction
    /// between them is about what the stream said; liveness is only about
    /// whether it said anything, and a heartbeat that valued events more highly
    /// would drop connections carrying long, quiet, entirely healthy jobs.
    ///
    /// # Errors
    ///
    /// Returns [`HeartbeatFault::ClockRegressed`].
    pub fn observe(
        &mut self,
        item: &StreamItem,
        now_milliseconds: u64,
    ) -> Result<ConnectionState, HeartbeatFault> {
        match item {
            StreamItem::Heartbeat | StreamItem::Event(_) => self.refresh(now_milliseconds)?,
        }
        Ok(ConnectionState::Healthy)
    }

    /// Records activity at `now_milliseconds`.
    ///
    /// # Errors
    ///
    /// Returns [`HeartbeatFault::ClockRegressed`].
    pub fn refresh(&mut self, now_milliseconds: u64) -> Result<(), HeartbeatFault> {
        self.require_monotonic(now_milliseconds)?;
        self.last_activity_milliseconds = now_milliseconds;
        Ok(())
    }

    /// Returns what this connection is at `now_milliseconds`.
    ///
    /// The boundary belongs to health. A gap of exactly the timeout is a stream
    /// that spoke exactly on time, and calling that dead would drop connections
    /// for being punctual.
    ///
    /// # Errors
    ///
    /// Returns [`HeartbeatFault::ClockRegressed`].
    pub fn state_at(&self, now_milliseconds: u64) -> Result<ConnectionState, HeartbeatFault> {
        self.require_monotonic(now_milliseconds)?;
        let quiet_for = now_milliseconds - self.last_activity_milliseconds;
        Ok(if quiet_for > self.timeout_milliseconds {
            ConnectionState::TimedOut
        } else {
            ConnectionState::Healthy
        })
    }

    /// Requires an instant not to precede the last one this heartbeat saw.
    fn require_monotonic(&self, now_milliseconds: u64) -> Result<(), HeartbeatFault> {
        if now_milliseconds < self.last_activity_milliseconds {
            return Err(HeartbeatFault::ClockRegressed {
                last: self.last_activity_milliseconds,
                named: now_milliseconds,
            });
        }
        Ok(())
    }
}
