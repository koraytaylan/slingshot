//! What the simulated author does, written down before it does it.
//!
//! A script is validated when it is built, so a test that asks for something
//! the contract cannot produce fails while it is being written rather than
//! halfway through a run. That is worth the ceremony: a transport suite spends
//! most of its time on cases that are hard to reach, and the worst outcome is a
//! test that silently exercises a different case than its name claims.

use std::collections::BTreeMap;

/// How the author answers one request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScriptedResponse {
    /// Answer with this status and body.
    Respond {
        /// The response body, as exact bytes.
        body: Vec<u8>,
        /// The response status.
        status: u16,
    },
    /// Accept the request and answer that the work was already accepted.
    AlreadyAccepted {
        /// The operation identifier the author already holds.
        agent_operation_identifier: String,
    },
    /// Refuse, because the request names a contract this author does not have.
    ContractDrift {
        /// Which field differs, for a test to assert on.
        field: String,
    },
    /// Close the connection without answering.
    CloseWithoutAnswering,
    /// Answer, then stop the event stream part way through.
    TruncateStream {
        /// How many events to emit before stopping.
        events_before_closing: usize,
    },
}

/// One thing the author is asked to do, and how it answers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptedExchange {
    /// The response this exchange produces.
    pub response: ScriptedResponse,
    /// The route it answers on.
    pub route: String,
}

/// Reason a script could not be built.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ScriptFailure {
    /// A route is not one this author serves.
    #[error("{route} is not a route this author serves")]
    UnknownRoute {
        /// The route that was asked for.
        route: String,
    },
    /// A route shaped like a publisher's was asked for.
    ///
    /// Refused at script-building time as well as at request time, because a
    /// suite that could write one would eventually write one by accident and
    /// then discover the refusal as a puzzling failure rather than a rule.
    #[error("{route} is a publisher route, and this author serves none")]
    PublisherRoute {
        /// The route that was asked for.
        route: String,
    },
    /// A script says nothing about what the author should do.
    #[error("a script says what the author does, and this one says nothing")]
    Empty,
}

/// Routes this author serves.
pub const AUTHOR_ROUTES: &[&str] = &[
    "/bin/slingshot/agent/capabilities",
    "/bin/slingshot/agent/submit",
    "/bin/slingshot/agent/events",
    "/bin/slingshot/agent/snapshot",
    "/bin/slingshot/agent/artifact",
    "/libs/granite/csrf/token",
];

/// Route prefixes a publisher would serve and this author never does.
pub const PUBLISHER_PREFIXES: &[&str] = &["/content/dam", "/etc.clientlibs", "/publish"];

/// Everything the author does, in the order it does it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Script {
    /// Exchanges still to be used, per route, oldest first.
    exchanges: BTreeMap<String, Vec<ScriptedResponse>>,
}

impl Script {
    /// Returns the script `exchanges` describe.
    ///
    /// # Errors
    ///
    /// Returns [`ScriptFailure`] naming the first thing the contract cannot do.
    pub fn of(exchanges: Vec<ScriptedExchange>) -> Result<Self, ScriptFailure> {
        if exchanges.is_empty() {
            return Err(ScriptFailure::Empty);
        }
        let mut held: BTreeMap<String, Vec<ScriptedResponse>> = BTreeMap::new();
        for exchange in exchanges {
            require_author_route(&exchange.route)?;
            held.entry(exchange.route).or_default().push(exchange.response);
        }
        Ok(Self { exchanges: held })
    }

    /// Takes what the author does next on `route`.
    pub fn next_on(&mut self, route: &str) -> Option<ScriptedResponse> {
        let queued = self.exchanges.get_mut(route)?;
        if queued.is_empty() { None } else { Some(queued.remove(0)) }
    }

    /// Returns whether every scripted exchange has been used.
    #[must_use]
    pub fn is_exhausted(&self) -> bool {
        self.exchanges.values().all(Vec::is_empty)
    }

    /// Returns how many exchanges are still to be used.
    #[must_use]
    pub fn remaining(&self) -> usize {
        self.exchanges.values().map(Vec::len).sum()
    }
}

/// Requires `route` to be one this author serves.
///
/// # Errors
///
/// Returns [`ScriptFailure::PublisherRoute`] or
/// [`ScriptFailure::UnknownRoute`].
pub fn require_author_route(route: &str) -> Result<(), ScriptFailure> {
    if PUBLISHER_PREFIXES.iter().any(|prefix| route.starts_with(prefix)) {
        return Err(ScriptFailure::PublisherRoute { route: route.to_owned() });
    }
    if !AUTHOR_ROUTES.contains(&route) {
        return Err(ScriptFailure::UnknownRoute { route: route.to_owned() });
    }
    Ok(())
}
