//! What one event does to the subscription, as opposed to any job.
//!
//! A subscription and a job advance at different rates and for different
//! reasons. One stream carries many jobs, so its cursor moves on every event
//! while any given job's sequence moves on only some of them, and an event
//! about work this daemon does not hold still moves the cursor. Folding both
//! through one function would make the subscription's progress depend on
//! whether a local row happened to exist, which is how a reconnection ends up
//! resuming from a position that skips other jobs' events.
//!
//! So this fold knows about four things and nothing else: which subscription,
//! which generation, which position, and what was at that position. It returns
//! the next state rather than changing one, because the caller decides whether
//! to keep the answer, and a fold that had already committed would have made
//! that decision for it.
//!
//! # Cursors are ordered
//!
//! The protocol issues cursors that increase, and this fold compares them
//! directly. That is a real assumption and worth naming: an agent that issued
//! unordered cursors would make its own stream unresumable, because there would
//! be no answer to the question of what "after this position" means.

use slingshot_domain::remote_job::EventStreamCursor;

/// What one event did to the subscription's own record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubscriptionDisposition {
    /// The subscription now sits at a later position.
    Advanced,
    /// The same position with the same contents, so nothing moved.
    ExactReplay,
    /// An earlier position, which is history rather than news.
    StaleCursorOnly,
    /// The same position with different contents, which nothing here can settle.
    IntegrityConflictNeedsReconciliation,
}

impl SubscriptionDisposition {
    /// Returns whether this disposition leaves the subscription usable.
    ///
    /// A conflict does not. Two accounts of one position mean the stream and
    /// the record disagree, and no amount of further streaming resolves that.
    #[must_use]
    pub fn permits_streaming(self) -> bool {
        !matches!(self, Self::IntegrityConflictNeedsReconciliation)
    }
}

/// Why one event is not about this subscription at all.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FoldRefusal {
    /// The event names another subscription.
    #[error("this fold is for one subscription, and this event names another")]
    AnotherSubscription,
    /// The event names another incarnation of the event store.
    #[error("this fold is at generation {held}, and this event names {named}")]
    AnotherGeneration {
        /// Which generation the fold is at.
        held: u64,
        /// Which generation the event names.
        named: u64,
    },
}

/// What one event says about the subscription that delivered it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubscriptionFact {
    /// Which incarnation of the store it came from.
    pub agent_event_store_generation: u64,
    /// What was at this position, canonically.
    pub canonical_digest: String,
    /// Which position it sits at.
    pub cursor: EventStreamCursor,
    /// Which subscription delivered it.
    pub daemon_subscription_identifier: String,
}

/// What one subscription has durably folded in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubscriptionFold {
    /// What was at the position folded in, when one has been.
    canonical_digest: Option<String>,
    /// The position folded in, when one has been.
    cursor: Option<EventStreamCursor>,
    /// Which incarnation of the store this fold is over.
    generation: u64,
    /// Which subscription this is.
    subscription: String,
}

impl SubscriptionFold {
    /// Returns a fold that has taken nothing in yet.
    #[must_use]
    pub fn opened(subscription: &str, generation: u64) -> Self {
        Self {
            canonical_digest: None,
            cursor: None,
            generation,
            subscription: subscription.to_owned(),
        }
    }

    /// Returns the position this fold sits at.
    #[must_use]
    pub fn cursor(&self) -> Option<&EventStreamCursor> {
        self.cursor.as_ref()
    }

    /// Returns what was at that position.
    #[must_use]
    pub fn canonical_digest(&self) -> Option<&str> {
        self.canonical_digest.as_deref()
    }

    /// Returns which incarnation of the store this fold is over.
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Returns what this fold becomes when `fact` is taken in.
    ///
    /// Pure: the answer includes the next fold, and taking it or leaving it is
    /// the caller's decision. A conflict returns the fold unchanged, because
    /// choosing between two accounts of one position is precisely what cannot
    /// be done from inside a fold over those accounts.
    ///
    /// # Errors
    ///
    /// Returns [`FoldRefusal::AnotherSubscription`] or
    /// [`FoldRefusal::AnotherGeneration`], neither of which is an event this
    /// fold has any business folding.
    pub fn folded(
        &self,
        fact: &SubscriptionFact,
    ) -> Result<(SubscriptionDisposition, Self), FoldRefusal> {
        if fact.daemon_subscription_identifier != self.subscription {
            return Err(FoldRefusal::AnotherSubscription);
        }
        if fact.agent_event_store_generation != self.generation {
            return Err(FoldRefusal::AnotherGeneration {
                held: self.generation,
                named: fact.agent_event_store_generation,
            });
        }
        let Some(held) = &self.cursor else {
            return Ok((SubscriptionDisposition::Advanced, self.taking(fact)));
        };
        if fact.cursor > *held {
            return Ok((SubscriptionDisposition::Advanced, self.taking(fact)));
        }
        if fact.cursor < *held {
            return Ok((SubscriptionDisposition::StaleCursorOnly, self.clone()));
        }
        if self.canonical_digest.as_deref() == Some(fact.canonical_digest.as_str()) {
            return Ok((SubscriptionDisposition::ExactReplay, self.clone()));
        }
        Ok((SubscriptionDisposition::IntegrityConflictNeedsReconciliation, self.clone()))
    }

    /// Returns this fold with `fact` taken in.
    fn taking(&self, fact: &SubscriptionFact) -> Self {
        Self {
            canonical_digest: Some(fact.canonical_digest.clone()),
            cursor: Some(fact.cursor.clone()),
            generation: self.generation,
            subscription: self.subscription.clone(),
        }
    }
}
