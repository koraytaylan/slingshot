//! The author, answering on loopback at a port the operating system picked.
//!
//! Loopback and ephemeral, so a suite can run several at once and none of them
//! can be reached from anywhere else. The port is asked for rather than chosen,
//! because a fixed port is a test that fails when something else is running.
//!
//! Every request is checked for a credential before the script is consulted, so
//! a scripted answer cannot accidentally serve an unauthenticated caller. What
//! gets recorded is whether a credential was presented and whether it was
//! accepted, never what it was.

use std::sync::{Arc, Mutex};

use crate::fake_author::recording::{CredentialKind, RecordedRequest, Recording};
use crate::fake_author::script::{PUBLISHER_PREFIXES, Script, ScriptedResponse};

/// How the author decides whether a caller may be served.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CredentialPolicy {
    /// A Basic header is required.
    Basic,
    /// A Bearer header is required.
    Bearer,
}

impl CredentialPolicy {
    /// Returns which kind of credential a header presents.
    #[must_use]
    pub fn kind_of(authorization: Option<&str>) -> CredentialKind {
        match authorization {
            Some(value) if value.starts_with("Basic ") => CredentialKind::Basic,
            Some(value) if value.starts_with("Bearer ") => CredentialKind::Bearer,
            _ => CredentialKind::Absent,
        }
    }

    /// Returns whether a header of this kind is the one required.
    ///
    /// Kind alone. Whether the value inside is right is the simulator's
    /// business elsewhere, and comparing it here would mean holding it.
    #[must_use]
    pub fn accepts(&self, kind: CredentialKind) -> bool {
        matches!(
            (self, kind),
            (Self::Basic, CredentialKind::Basic) | (Self::Bearer, CredentialKind::Bearer)
        )
    }
}

/// What one request carried, as the author sees it before answering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncomingRequest {
    /// The authorization header, if there was one.
    pub authorization: Option<String>,
    /// The partition the request names, when it names one.
    pub author_target_identity_digest: Option<String>,
    /// The route it asks for.
    pub route: String,
    /// The environment revision it names, when it names one.
    pub selected_environment_revision: Option<String>,
}

/// How the author answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    /// This status and body.
    Responded {
        /// The body.
        body: Vec<u8>,
        /// The status.
        status: u16,
    },
    /// Nothing; the connection closed.
    Closed,
    /// The route is not one this author serves.
    RouteRefused,
    /// No credential of the required kind was presented.
    Unauthenticated,
    /// The script says nothing further about this route.
    ScriptExhausted,
}

/// One simulated author.
#[derive(Debug)]
pub struct FakeAuthor {
    /// What it does, in order.
    script: Mutex<Script>,
    /// Which credential it requires.
    policy: CredentialPolicy,
    /// What it has been asked, and what it did.
    recording: Arc<Recording>,
}

impl FakeAuthor {
    /// Returns an author following `script` and requiring `policy`.
    #[must_use]
    pub fn following(script: Script, policy: CredentialPolicy) -> Self {
        Self { script: Mutex::new(script), policy, recording: Arc::new(Recording::new()) }
    }

    /// Returns what this author has been asked.
    #[must_use]
    pub fn recording(&self) -> Arc<Recording> {
        Arc::clone(&self.recording)
    }

    /// Answers one request.
    ///
    /// The order is deliberate. A publisher-shaped route is refused and
    /// recorded before anything else, because a client asking for one is a
    /// finding whatever else is true of the request. Then the credential, so a
    /// scripted answer can never serve an unauthenticated caller. Only then the
    /// script.
    pub fn answer(&self, request: &IncomingRequest) -> Answer {
        if PUBLISHER_PREFIXES.iter().any(|prefix| request.route.starts_with(prefix)) {
            self.recording.refused_route(&request.route);
            return Answer::RouteRefused;
        }
        let kind = CredentialPolicy::kind_of(request.authorization.as_deref());
        let accepted = self.policy.accepts(kind);
        self.recording.served(RecordedRequest {
            credential_accepted: accepted,
            credential_kind: kind,
            author_target_identity_digest: request.author_target_identity_digest.clone(),
            route: request.route.clone(),
            selected_environment_revision: request.selected_environment_revision.clone(),
        });
        if !accepted {
            return Answer::Unauthenticated;
        }
        let scripted = self.script.lock().ok().and_then(|mut held| held.next_on(&request.route));
        match scripted {
            None => Answer::ScriptExhausted,
            Some(ScriptedResponse::Respond { body, status }) => Answer::Responded { body, status },
            Some(ScriptedResponse::CloseWithoutAnswering) => Answer::Closed,
            Some(ScriptedResponse::AlreadyAccepted { agent_operation_identifier }) => {
                Answer::Responded {
                    body: format!("{{\"already_accepted\":\"{agent_operation_identifier}\"}}")
                        .into_bytes(),
                    status: ALREADY_ACCEPTED_STATUS,
                }
            }
            Some(ScriptedResponse::ContractDrift { field }) => Answer::Responded {
                body: format!("{{\"contract_drift\":\"{field}\"}}").into_bytes(),
                status: CONFLICT_STATUS,
            },
            Some(ScriptedResponse::TruncateStream { events_before_closing }) => Answer::Responded {
                body: format!("{{\"events\":{events_before_closing},\"truncated\":true}}")
                    .into_bytes(),
                status: OK_STATUS,
            },
        }
    }

    /// Returns whether every scripted exchange has been used.
    #[must_use]
    pub fn script_is_exhausted(&self) -> bool {
        self.script.lock().map(|held| held.is_exhausted()).unwrap_or(true)
    }
}

/// The status a served request answers with.
pub const OK_STATUS: u16 = 200;

/// The status an already-accepted submission answers with.
pub const ALREADY_ACCEPTED_STATUS: u16 = 202;

/// The status a contract disagreement answers with.
pub const CONFLICT_STATUS: u16 = 409;
