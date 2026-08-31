//! What a client is told while work runs, and what asking it to stop means.
//!
//! Progress is a report about durable state rather than a stream this server
//! invents, and a cancellation ends a client's interest rather than the work.
//! Both follow from the same fact: the operation is the daemon's, and this
//! server is one of possibly several things watching it.
//!
//! # Cancelling cancels nothing remote
//!
//! A client that cancels stops receiving an answer. The operation it started
//! keeps running, because a client walking away is not a decision about work
//! that may already have changed an author, and because another client - or the
//! same one, reconnected - can still ask how it went. A server that cancelled
//! the work would make "stop telling me" mean "undo it if you can", which are
//! not the same request and cannot be distinguished afterwards.
//!
//! # Progress never goes backwards
//!
//! Reports arrive from a durable sequence that can repeat after a reconnect. A
//! repeat is dropped rather than forwarded, because a client watching a
//! progress bar go backwards learns something false about the work.

use std::collections::BTreeMap;

/// What one request is being told, and how much of it has been told.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Watch {
    /// The token a client correlates progress with.
    pub progress_token: String,
    /// The highest durable revision this client has been told about.
    pub reported_revision: u64,
}

/// What happened to one progress report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reported {
    /// It was forwarded, because it says something new.
    Forwarded,
    /// It was dropped, because it repeats or precedes what was said.
    Dropped,
    /// It was dropped, because nobody is listening any more.
    Detached,
}

/// What a cancellation did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cancelled {
    /// The answer is suppressed and the waiter detached.
    Detached,
    /// There was nothing under that identifier to detach.
    Unknown,
}

/// Who is watching what, and how far each has been told.
#[derive(Debug, Default)]
pub struct ProgressRegistry {
    /// One watch per active request identifier.
    watching: BTreeMap<String, Watch>,
    /// How many times anything remote was asked to stop.
    remote_cancellations: usize,
}

impl ProgressRegistry {
    /// Returns a registry watching nothing.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns how many requests are being watched.
    #[must_use]
    pub fn watching(&self) -> usize {
        self.watching.len()
    }

    /// Returns how many times this server asked anything remote to stop.
    ///
    /// Zero. The count exists so a suite can insist on it rather than trust a
    /// sentence in a comment.
    #[must_use]
    pub fn remote_cancellations(&self) -> usize {
        self.remote_cancellations
    }

    /// Begins watching one request under the token it correlates with.
    pub fn attach(&mut self, request_identifier: &str, progress_token: &str, from_revision: u64) {
        self.watching.insert(
            request_identifier.to_owned(),
            Watch { progress_token: progress_token.to_owned(), reported_revision: from_revision },
        );
    }

    /// Returns what happens to one report about one request.
    pub fn report(&mut self, request_identifier: &str, revision: u64) -> Reported {
        let Some(watch) = self.watching.get_mut(request_identifier) else {
            return Reported::Detached;
        };
        if revision <= watch.reported_revision {
            return Reported::Dropped;
        }
        watch.reported_revision = revision;
        Reported::Forwarded
    }

    /// Ends one client's interest in one request.
    ///
    /// Nothing remote is asked to stop, and the count of such requests stays
    /// where it is.
    pub fn cancel(&mut self, request_identifier: &str) -> Cancelled {
        match self.watching.remove(request_identifier) {
            Some(_) => Cancelled::Detached,
            None => Cancelled::Unknown,
        }
    }

    /// Ends every client's interest, after output has failed or input has ended.
    ///
    /// Idempotent: a second call finds nothing to detach and says so by
    /// returning an empty list rather than by failing.
    pub fn detach_all(&mut self) -> Vec<String> {
        let held: Vec<String> = self.watching.keys().cloned().collect();
        self.watching.clear();
        held
    }

    /// Returns the token one request's progress is correlated with.
    #[must_use]
    pub fn token_of(&self, request_identifier: &str) -> Option<&str> {
        self.watching.get(request_identifier).map(|watch| watch.progress_token.as_str())
    }
}
