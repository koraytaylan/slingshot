//! Which requests are in flight, and what each of them holds.
//!
//! One request is one entry: the identifier the client sent and how far its
//! answer has got. Holding that here rather than in the transport is what lets
//! a reconnected client be answered about work it started before the
//! connection dropped.
//!
//! # An identifier is reusable only once it names nothing
//!
//! A response that has been queued has not been delivered. Releasing its
//! identifier then would let a client reuse it for a second request while the
//! first answer is still on its way out, and the two answers would be
//! indistinguishable. So a reservation is held until the one writer says the
//! line went out in full - or, for a cancelled request, until the answer is
//! suppressed and every waiter is detached.
//!
//! # A duplicate disturbs nothing
//!
//! A second request under an identifier already in flight is refused, and the
//! original is neither replaced, dispatched again, nor detached. The client
//! made a mistake; the work it already started is not part of that mistake.

use std::collections::BTreeMap;

/// How many requests may be in flight at once.
pub const MAXIMUM_ACTIVE_REQUESTS: usize = 64;

/// How far one active request has got.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Standing {
    /// Reserved, and being handled.
    Reserved,
    /// Answered, with the answer queued and its last byte still unwritten.
    Answered,
    /// Cancelled, with the answer suppressed and waiters detaching.
    Cancelling,
}

/// Why a request was not admitted.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AdmissionRefusal {
    /// An identifier already in flight was used again.
    #[error("{0} names a request already in flight, which this one does not replace")]
    Duplicate(String),
    /// As many requests are in flight as this server admits.
    #[error("this server handles {MAXIMUM_ACTIVE_REQUESTS} requests at once")]
    Saturated,
}

/// What is in flight, and what each entry is doing.
#[derive(Debug, Default)]
pub struct ActiveRequestRegistry {
    /// One entry per identifier in flight.
    held: BTreeMap<String, Standing>,
    /// How many entries were released, for the suite to compare against.
    released: usize,
}

impl ActiveRequestRegistry {
    /// Returns a registry holding nothing.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns how many requests are in flight.
    #[must_use]
    pub fn active(&self) -> usize {
        self.held.len()
    }

    /// Returns how many entries have been released.
    #[must_use]
    pub fn released(&self) -> usize {
        self.released
    }

    /// Returns how far one request has got, when it is in flight.
    #[must_use]
    pub fn standing(&self, identifier: &str) -> Option<Standing> {
        self.held.get(identifier).copied()
    }

    /// Reserves one identifier before its handler is created.
    ///
    /// The reservation happens first, so a request this registry refuses is
    /// refused before anything is dispatched on its behalf.
    ///
    /// # Errors
    ///
    /// Returns [`AdmissionRefusal::Duplicate`] for an identifier already in
    /// flight and [`AdmissionRefusal::Saturated`] at the bound.
    pub fn reserve(&mut self, identifier: &str) -> Result<(), AdmissionRefusal> {
        if self.held.contains_key(identifier) {
            return Err(AdmissionRefusal::Duplicate(identifier.to_owned()));
        }
        if self.held.len() >= MAXIMUM_ACTIVE_REQUESTS {
            return Err(AdmissionRefusal::Saturated);
        }
        self.held.insert(identifier.to_owned(), Standing::Reserved);
        Ok(())
    }

    /// Records that one request's answer has been queued.
    ///
    /// The reservation is retained: a queued answer is not a delivered one.
    pub fn answered(&mut self, identifier: &str) {
        if let Some(standing) = self.held.get_mut(identifier)
            && *standing == Standing::Reserved
        {
            *standing = Standing::Answered;
        }
    }

    /// Records that one request's answer went out in full, and releases it.
    ///
    /// The only ordinary way an identifier becomes reusable.
    pub fn acknowledged(&mut self, identifier: &str) -> bool {
        if self.held.get(identifier) != Some(&Standing::Answered) {
            return false;
        }
        self.held.remove(identifier);
        self.released += 1;
        true
    }

    /// Records that a client asked to stop caring about one request.
    ///
    /// Nothing remote is asked to stop. The answer is suppressed and the
    /// waiters detach; the operation this request started keeps running,
    /// because a client walking away is not a decision about the work.
    pub fn cancelling(&mut self, identifier: &str) -> bool {
        let Some(standing) = self.held.get_mut(identifier) else {
            return false;
        };
        *standing = Standing::Cancelling;
        true
    }

    /// Releases one cancelled request, once suppression and detachment are done.
    pub fn cancelled(&mut self, identifier: &str) -> bool {
        if self.held.get(identifier) != Some(&Standing::Cancelling) {
            return false;
        }
        self.held.remove(identifier);
        self.released += 1;
        true
    }

    /// Releases everything, after end of input or an output failure.
    ///
    /// Returns what was released, so the caller can detach exactly those
    /// waiters and no others.
    pub fn release_all(&mut self) -> Vec<String> {
        let held: Vec<String> = self.held.keys().cloned().collect();
        self.released += held.len();
        self.held.clear();
        held
    }
}
