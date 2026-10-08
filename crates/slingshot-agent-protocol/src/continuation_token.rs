//! A continuation token, and why the agent cannot simply trust one.
//!
//! A token says "resume where you were". Two things follow. It has to carry
//! enough to identify that place exactly, because resuming somewhere else is
//! worse than starting over. And it has to be unforgeable, because the place it
//! names is inside somebody's data.
//!
//! So a token is state plus a digest over that state and a key the agent holds.
//! A client cannot construct one, cannot alter one without invalidating it, and
//! cannot carry one from one query to another - the query it belongs to is part
//! of what is signed.
//!
//! Precedence when several things are wrong at once is fixed rather than
//! whichever check ran first. Integrity comes before staleness, because a
//! tampered token is a different finding from an expired one and reporting the
//! milder of the two would hide the other.

use sha2::{Digest as _, Sha256};
use slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract;
use slingshot_domain::command::result_window::maximum_result_limit;

use crate::continuation_key_authority::{KeyRing, ValidatingKey};

/// Version marker every continuation token is derived under.
pub const TOKEN_VERSION: &str = "slingshot.agent-continuation/2";

/// Separator between the fields a token digest is taken over.
pub const FIELD_SEPARATOR: u8 = 0;

/// What a token says about where to resume.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContinuationState {
    /// The partition the query ran in.
    pub author_target_identity_digest: String,
    /// Which incarnation of the store it ran against.
    pub agent_event_store_generation: u64,
    /// The exact query it belongs to.
    pub query_digest: String,
    /// Where in that query's results to resume.
    pub position: u64,
    /// The maximum page size fixed when enumeration began, retained by successors.
    pub initial_result_limit: u64,
    /// When this token stops being honoured.
    pub expires_at_unix_milliseconds: u64,
}

/// Why a token was not honoured.
///
/// In precedence order, and the order matters: a tampered token is a different
/// finding from an expired one, and reporting the milder would hide the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, thiserror::Error)]
pub enum ContinuationRefusal {
    /// The token is not the shape a token has.
    #[error("this is not a continuation token")]
    Malformed,
    /// No key this agent holds signs this token.
    #[error("no key this agent holds signs this token")]
    IntegrityInvalid,
    /// The token belongs to another partition.
    #[error("this token belongs to another author target")]
    WrongTarget,
    /// The token belongs to another query.
    #[error("this token belongs to another query")]
    WrongQuery,
    /// The store was rebuilt since the token was issued.
    #[error("this token was issued against another incarnation of the event store")]
    WrongGeneration,
    /// The token has expired.
    #[error("this token has expired")]
    Expired,
}

/// One continuation token, as it crosses the wire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContinuationToken {
    /// The digest binding the state to a key.
    integrity: String,
    /// What the token says.
    state: ContinuationState,
}

impl ContinuationToken {
    /// Returns the token `state` produces under `key`.
    #[must_use]
    pub fn issue(state: ContinuationState, key: &str) -> Self {
        let integrity = integrity_of(&state, key);
        Self { integrity, state }
    }

    /// Returns what this token says, without having checked it.
    ///
    /// Named so a caller cannot mistake it for a validated value: reading a
    /// token is not the same as honouring one.
    #[must_use]
    pub fn unvalidated_state(&self) -> &ContinuationState {
        &self.state
    }

    /// Returns how many bytes this token occupies.
    #[must_use]
    pub fn state_bytes(&self) -> usize {
        self.integrity.len()
            + self.state.author_target_identity_digest.len()
            + self.state.query_digest.len()
    }

    /// Returns whether token state and its original page size fit the declared bounds.
    #[must_use]
    pub fn is_bounded(&self) -> bool {
        let allowed = AuthorAgentTransportContract::embedded()
            .limit("maximum_agent_continuation_key_state_bytes");
        u64::try_from(self.state_bytes()).unwrap_or(u64::MAX) <= allowed
            && self.state.initial_result_limit > 0
            && self.state.initial_result_limit <= maximum_result_limit()
    }

    /// Honours this token, or says why it is not honoured.
    ///
    /// Integrity first, so a tampered token never reaches a comparison against
    /// data it named - a token that could steer a target or query check before
    /// being shown to be forged would be a token doing exactly what forging one
    /// is for.
    ///
    /// # Errors
    ///
    /// Returns [`ContinuationRefusal`] naming the first thing that is wrong, in
    /// a fixed precedence rather than whichever check ran first.
    pub fn validate(
        &self,
        ring: &KeyRing,
        expected_target: &str,
        expected_query_digest: &str,
        expected_generation: u64,
        now_unix_milliseconds: u64,
    ) -> Result<ValidatingKey, ContinuationRefusal> {
        if !self.is_bounded() {
            return Err(ContinuationRefusal::Malformed);
        }
        let signing = [Some(ring.current.as_str()), ring.prior.as_deref()]
            .into_iter()
            .flatten()
            .find(|key| integrity_of(&self.state, key) == self.integrity)
            .ok_or(ContinuationRefusal::IntegrityInvalid)?;
        let key = ring
            .validating(signing, now_unix_milliseconds)
            .ok_or(ContinuationRefusal::IntegrityInvalid)?;
        if self.state.author_target_identity_digest != expected_target {
            return Err(ContinuationRefusal::WrongTarget);
        }
        if self.state.query_digest != expected_query_digest {
            return Err(ContinuationRefusal::WrongQuery);
        }
        if self.state.agent_event_store_generation != expected_generation {
            return Err(ContinuationRefusal::WrongGeneration);
        }
        if now_unix_milliseconds >= self.state.expires_at_unix_milliseconds {
            return Err(ContinuationRefusal::Expired);
        }
        Ok(key)
    }
}

/// Returns the digest binding `state` to `key`.
fn integrity_of(state: &ContinuationState, key: &str) -> String {
    let mut hasher = Sha256::new();
    for field in [
        TOKEN_VERSION.as_bytes(),
        key.as_bytes(),
        state.author_target_identity_digest.as_bytes(),
        state.query_digest.as_bytes(),
        &state.agent_event_store_generation.to_be_bytes(),
        &state.position.to_be_bytes(),
        &state.initial_result_limit.to_be_bytes(),
        &state.expires_at_unix_milliseconds.to_be_bytes(),
    ] {
        hasher.update(field);
        hasher.update([FIELD_SEPARATOR]);
    }
    hasher.finalize().iter().map(|octet| format!("{octet:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::{ContinuationRefusal, ContinuationState, ContinuationToken};
    use crate::continuation_key_authority::{KeyRing, ValidatingKey};
    use slingshot_domain::command::result_window::maximum_result_limit;

    const DIGEST_PAIRS: usize = 32;
    const EXPIRY: u64 = 10;

    fn token(limit: u64) -> ContinuationToken {
        ContinuationToken::issue(
            ContinuationState {
                author_target_identity_digest: "1d".repeat(DIGEST_PAIRS),
                agent_event_store_generation: 1,
                query_digest: "40".repeat(DIGEST_PAIRS),
                position: 1,
                initial_result_limit: limit,
                expires_at_unix_milliseconds: EXPIRY,
            },
            "synthetic signing key",
        )
    }

    fn validate(token: &ContinuationToken) -> Result<ValidatingKey, ContinuationRefusal> {
        token.validate(
            &KeyRing::initial("synthetic signing key"),
            &"1d".repeat(DIGEST_PAIRS),
            &"40".repeat(DIGEST_PAIRS),
            1,
            1,
        )
    }

    #[test]
    fn changing_only_the_original_limit_invalidates_the_signature() {
        let mut forged = token(1);
        forged.state.initial_result_limit += 1;
        assert_eq!(validate(&forged), Err(ContinuationRefusal::IntegrityInvalid));
    }

    #[test]
    fn original_limit_accepts_the_contract_boundary_and_refuses_both_outside_values() {
        assert_eq!(validate(&token(1)), Ok(ValidatingKey::Current));
        assert_eq!(validate(&token(maximum_result_limit())), Ok(ValidatingKey::Current));
        assert_eq!(validate(&token(0)), Err(ContinuationRefusal::Malformed));
        assert_eq!(
            validate(&token(maximum_result_limit() + 1)),
            Err(ContinuationRefusal::Malformed)
        );
    }

    #[test]
    fn version_two_signature_matches_the_independent_cross_language_vector() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/continuation-key-lifecycle/token-v2.json"
        ))
        .expect("an independently calculated six-field token");
        let state = &fixture["state"];
        let issued = ContinuationToken::issue(
            ContinuationState {
                author_target_identity_digest: state["author_target_identity_digest"]
                    .as_str()
                    .expect("a target digest")
                    .to_owned(),
                agent_event_store_generation: state["agent_event_store_generation"]
                    .as_u64()
                    .expect("a generation"),
                query_digest: state["query_digest"].as_str().expect("a query digest").to_owned(),
                position: state["position"].as_u64().expect("a position"),
                initial_result_limit: state["initial_result_limit"]
                    .as_u64()
                    .expect("an original limit"),
                expires_at_unix_milliseconds: state["expires_at_unix_milliseconds"]
                    .as_u64()
                    .expect("an expiry"),
            },
            "a key this agent holds",
        );
        assert_eq!(Some(issued.integrity.as_str()), fixture["integrity"].as_str());
    }
}
