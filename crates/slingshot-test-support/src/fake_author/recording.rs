//! What the author was asked, kept for a test to read, minus the secrets.
//!
//! A recording says whether a credential was presented and whether it was
//! accepted, and never what it was. That is not squeamishness: a test harness
//! that retained credential values would put them in assertion output, in
//! failure messages, and in whatever a continuous-integration system keeps -
//! and the suite that proves secrets do not leak would be the thing leaking
//! them.

use std::collections::BTreeMap;
use std::sync::Mutex;

/// Which kind of credential a request presented.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CredentialKind {
    /// A Basic authorization header.
    Basic,
    /// A Bearer authorization header.
    Bearer,
    /// No authorization header at all.
    Absent,
}

/// What one request carried, without carrying it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedRequest {
    /// Whether the credential was accepted.
    pub credential_accepted: bool,
    /// Which kind of credential it presented.
    pub credential_kind: CredentialKind,
    /// The partition the request named, when it named one.
    pub author_target_identity_digest: Option<String>,
    /// The route it asked for.
    pub route: String,
    /// The environment revision it named, when it named one.
    pub selected_environment_revision: Option<String>,
}

/// Everything the author has been asked, and what it did about it.
#[derive(Debug, Default)]
pub struct Recording {
    /// Requests, in the order they arrived.
    requests: Mutex<Vec<RecordedRequest>>,
    /// Routes outside the author contract that something tried.
    refused_routes: Mutex<Vec<String>>,
    /// How many logical effects each operation has had.
    effects: Mutex<BTreeMap<String, u32>>,
}

impl Recording {
    /// Returns an empty recording.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records one request the author served.
    pub fn served(&self, request: RecordedRequest) {
        if let Ok(mut held) = self.requests.lock() {
            held.push(request);
        }
    }

    /// Records one route outside the author contract that something tried.
    ///
    /// Kept separately from served requests, because "somebody asked for a
    /// publisher route" is a finding about the client rather than a thing the
    /// author did.
    pub fn refused_route(&self, route: &str) {
        if let Ok(mut held) = self.refused_routes.lock() {
            held.push(route.to_owned());
        }
    }

    /// Records that one operation had a logical effect.
    pub fn had_effect(&self, agent_operation_identifier: &str) {
        if let Ok(mut held) = self.effects.lock() {
            *held.entry(agent_operation_identifier.to_owned()).or_default() += 1;
        }
    }

    /// Returns every request served, in order.
    #[must_use]
    pub fn requests(&self) -> Vec<RecordedRequest> {
        self.requests.lock().map(|held| held.clone()).unwrap_or_default()
    }

    /// Returns every route outside the contract something tried.
    #[must_use]
    pub fn refused_routes(&self) -> Vec<String> {
        self.refused_routes.lock().map(|held| held.clone()).unwrap_or_default()
    }

    /// Returns how many logical effects one operation has had.
    ///
    /// The number a whole transport suite exists to keep at one, however many
    /// physical records, retries, and reconnections happened around it.
    #[must_use]
    pub fn effects(&self, agent_operation_identifier: &str) -> u32 {
        self.effects
            .lock()
            .ok()
            .and_then(|held| held.get(agent_operation_identifier).copied())
            .unwrap_or_default()
    }

    /// Returns whether any recorded value looks like a credential.
    ///
    /// Asserted by the suite that uses this, so a later change that started
    /// keeping one is caught by the harness rather than by a reader.
    #[must_use]
    pub fn holds_no_credential_values(&self) -> bool {
        let rendered = format!("{:?}{:?}", self.requests(), self.refused_routes());
        !["Bearer ", "Basic ", "password", "secret", "token="]
            .iter()
            .any(|marker| rendered.contains(marker))
    }
}
