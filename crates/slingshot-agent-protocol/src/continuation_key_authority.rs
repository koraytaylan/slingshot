//! The one authority every deployment provides, whatever it is running on.
//!
//! A continuation token is only meaningful if the key that signed it is still
//! the key the far side has, and that is a question about durable shared state
//! rather than about one process. So the contract is a small linearizable
//! store: read a key, write it only if it still holds what you expected, and
//! only while you hold the lease.
//!
//! Every deployment implements all of it. A single instance is not permitted a
//! cheaper version, because the guarantees would then change the day somebody
//! added a node - and the code depending on them would not know. Nothing here
//! observes node count, and nothing branches on which deployment it is.
//!
//! Keys rotate with the previous one retained. A token issued a moment before
//! a rotation is still a token somebody is holding, so the prior key outlives
//! the longest token issued under it plus the skew two clocks may differ by,
//! and validation tries the current key and then the prior one.

use slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract;

/// Which key a validation succeeded under.
///
/// Reported rather than hidden, because "this token is valid under the prior
/// key" is the signal that a rotation is in progress and a client should
/// expect a new one - not merely an internal detail of how it was checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidatingKey {
    /// The key issued now.
    Current,
    /// The key retained from before the last rotation.
    Prior,
}

/// Which keys an authority holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyRing {
    /// The key tokens are issued under now.
    pub current: String,
    /// The key retained from before the last rotation, while it lives.
    pub prior: Option<String>,
    /// When the prior key stops being accepted.
    pub prior_expires_at_unix_milliseconds: u64,
}

/// Reason a key ring could not be read, written, or rotated.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum KeyRingFailure {
    /// The deployment holds no key ring.
    ///
    /// Not the same as an empty one. A caller finding an empty ring would
    /// issue keys into it; one finding nothing is told to look at why, because
    /// the ring not existing is a deployment that was never prepared.
    #[error("this deployment holds no continuation key ring, and one is not created implicitly")]
    Absent,
    /// One key is beyond its bound.
    #[error("a continuation key state holds at most {allowed} bytes, and this holds {actual}")]
    KeyTooLong {
        /// How long it may be.
        allowed: u64,
        /// How long it was.
        actual: usize,
    },
    /// The whole ring is beyond its bound.
    #[error("a key ring record holds at most {allowed} bytes, and this holds {actual}")]
    RecordTooLong {
        /// How long it may be.
        allowed: u64,
        /// How long it was.
        actual: usize,
    },
    /// A rotation was asked for while the previous one is still retained.
    #[error(
        "the previous key is retained until {retained_until}, and rotating now would strand \
             every token issued under it"
    )]
    PriorStillRetained {
        /// When the previous key stops being accepted.
        retained_until: u64,
    },
}

impl KeyRing {
    /// Returns a ring holding one key and nothing retained.
    #[must_use]
    pub fn initial(current: &str) -> Self {
        Self { current: current.to_owned(), prior: None, prior_expires_at_unix_milliseconds: 0 }
    }

    /// Returns how many bytes this ring occupies.
    #[must_use]
    pub fn record_bytes(&self) -> usize {
        self.current.len() + self.prior.as_deref().unwrap_or_default().len()
    }

    /// Requires this ring to fit the bounds the transport contract names.
    ///
    /// # Errors
    ///
    /// Returns [`KeyRingFailure::KeyTooLong`] or
    /// [`KeyRingFailure::RecordTooLong`].
    pub fn require_bounded(&self) -> Result<(), KeyRingFailure> {
        let contract = AuthorAgentTransportContract::embedded();
        let key = contract.limit("maximum_agent_continuation_key_state_bytes");
        let record = contract.limit("maximum_continuation_key_authority_record_bytes");
        for held in [Some(self.current.as_str()), self.prior.as_deref()].into_iter().flatten() {
            if u64::try_from(held.len()).unwrap_or(u64::MAX) > key {
                return Err(KeyRingFailure::KeyTooLong { allowed: key, actual: held.len() });
            }
        }
        if u64::try_from(self.record_bytes()).unwrap_or(u64::MAX) > record {
            return Err(KeyRingFailure::RecordTooLong {
                allowed: record,
                actual: self.record_bytes(),
            });
        }
        Ok(())
    }

    /// Returns which key validates `presented`, when either does.
    ///
    /// Current first, then prior. A token that only the prior key validates is
    /// one issued before the last rotation and still inside its retention,
    /// which is a token to honour rather than a token to refuse.
    #[must_use]
    pub fn validating(&self, presented: &str, now_unix_milliseconds: u64) -> Option<ValidatingKey> {
        if presented == self.current {
            return Some(ValidatingKey::Current);
        }
        let retained = self.prior.as_deref()?;
        let alive = now_unix_milliseconds < self.prior_expires_at_unix_milliseconds;
        (presented == retained && alive).then_some(ValidatingKey::Prior)
    }

    /// Returns this ring with `next` current and the old key retained.
    ///
    /// Refused while a previous rotation's key is still retained, because two
    /// rotations inside one retention window would strand every token issued
    /// under the key that fell off the end.
    ///
    /// # Errors
    ///
    /// Returns [`KeyRingFailure::PriorStillRetained`], or a bound refusal.
    pub fn rotated(&self, next: &str, now_unix_milliseconds: u64) -> Result<Self, KeyRingFailure> {
        if self.prior.is_some() && now_unix_milliseconds < self.prior_expires_at_unix_milliseconds {
            return Err(KeyRingFailure::PriorStillRetained {
                retained_until: self.prior_expires_at_unix_milliseconds,
            });
        }
        let retention = AuthorAgentTransportContract::embedded()
            .limit("continuation_key_prior_retention_milliseconds");
        let rotated = Self {
            current: next.to_owned(),
            prior: Some(self.current.clone()),
            prior_expires_at_unix_milliseconds: now_unix_milliseconds.saturating_add(retention),
        };
        rotated.require_bounded()?;
        Ok(rotated)
    }
}
