//! Who is asking the agent, and which operation they mean.
//!
//! Two identifiers, both derived rather than allocated. A daemon's subscription
//! and an operation's name at the agent are both digests over the facts that
//! decide them, so two daemons resolving the same target agree without
//! coordinating, and one daemon that restarts arrives at the same names it had.
//!
//! Both are partitioned by the opaque author-target digest and the selected
//! environment revision, so the same operation identifier under two security
//! contexts is two operations at the agent as well as two rows locally. The
//! target digest is used exactly as Plan 0002 produced it - never re-hashed,
//! and never hashed from its rendering - because a second digest would be a
//! second identity for one thing, and the two would eventually disagree.
//!
//! Generation is part of both. An agent event store that was rebuilt has a new
//! generation, and identifiers from before it are not identifiers from after
//! it: a subscription that survived a rebuild would be a subscription to a
//! stream that no longer means what it meant.

use sha2::{Digest as _, Sha256};

/// Separator between the fields an agent identifier is derived from.
pub const FIELD_SEPARATOR: u8 = 0;

/// Version marker every daemon subscription identifier is derived under.
pub const SUBSCRIPTION_IDENTIFIER_VERSION: &str = "slingshot.daemon-subscription/1";

/// Version marker every agent operation identifier is derived under.
pub const OPERATION_IDENTIFIER_VERSION: &str = "slingshot.agent-operation/1";

/// Characters a derived identifier is spelled with.
pub const IDENTIFIER_CHARACTERS: usize = 64;

/// The largest generation an agent event store may reach.
///
/// Bounded so exhaustion is a fact this daemon can report rather than a wrap
/// that silently reuses identifiers from a store that no longer exists.
pub const MAXIMUM_GENERATION: u64 = u64::MAX - 1;

/// Reason an agent identity could not be read or advanced.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AgentIdentityFailure {
    /// A spelling is not a derived identifier.
    #[error("an agent identifier is {IDENTIFIER_CHARACTERS} lowercase hexadecimal characters")]
    NotCanonical,
    /// The event store has used every generation it may.
    #[error("this agent event store has reached generation {MAXIMUM_GENERATION}, its last")]
    GenerationsExhausted,
}

/// Which incarnation of an agent's event store this is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AgentEventStoreGeneration {
    /// The generation, counting from one.
    value: u64,
}

impl AgentEventStoreGeneration {
    /// Returns the first generation an event store has.
    #[must_use]
    pub fn first() -> Self {
        Self { value: 1 }
    }

    /// Returns the generation `value` names.
    #[must_use]
    pub fn of(value: u64) -> Self {
        Self { value }
    }

    /// Returns this generation as a number.
    #[must_use]
    pub fn value(self) -> u64 {
        self.value
    }

    /// Returns the generation after this one.
    ///
    /// # Errors
    ///
    /// Returns [`AgentIdentityFailure::GenerationsExhausted`] at the last one,
    /// rather than wrapping into identifiers a previous store already used.
    pub fn next(self) -> Result<Self, AgentIdentityFailure> {
        if self.value >= MAXIMUM_GENERATION {
            return Err(AgentIdentityFailure::GenerationsExhausted);
        }
        Ok(Self { value: self.value + 1 })
    }
}

/// Returns `octets` in lowercase hexadecimal.
fn render(octets: &[u8]) -> String {
    octets.iter().map(|octet| format!("{octet:02x}")).collect()
}

/// Returns whether `spelling` is a derived identifier.
fn is_canonical(spelling: &str) -> bool {
    spelling.len() == IDENTIFIER_CHARACTERS
        && spelling.bytes().all(|octet| octet.is_ascii_digit() || (b'a'..=b'f').contains(&octet))
}

/// Absorbs one length-separated field into a digest.
fn absorb(hasher: &mut Sha256, field: &[u8]) {
    hasher.update(field);
    hasher.update([FIELD_SEPARATOR]);
}

/// What one daemon's subscription to one agent's events is called.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DaemonSubscriptionIdentifier {
    /// The digest, in lowercase hexadecimal.
    spelling: String,
}

impl DaemonSubscriptionIdentifier {
    /// Returns the subscription one daemon has to one agent's events.
    ///
    /// The installation is in it, so two daemons serving one target from two
    /// installations subscribe separately; the generation is in it, so neither
    /// subscription survives the store being rebuilt.
    #[must_use]
    pub fn derive(
        installation_identifier: &str,
        author_target_identity_digest: &str,
        selected_environment_revision: &str,
        generation: AgentEventStoreGeneration,
    ) -> Self {
        let mut hasher = Sha256::new();
        absorb(&mut hasher, SUBSCRIPTION_IDENTIFIER_VERSION.as_bytes());
        absorb(&mut hasher, installation_identifier.as_bytes());
        absorb(&mut hasher, author_target_identity_digest.as_bytes());
        absorb(&mut hasher, selected_environment_revision.as_bytes());
        absorb(&mut hasher, &generation.value().to_be_bytes());
        Self { spelling: render(&hasher.finalize()) }
    }

    /// Returns the subscription `spelling` names.
    ///
    /// # Errors
    ///
    /// Returns [`AgentIdentityFailure::NotCanonical`].
    pub fn parse(spelling: &str) -> Result<Self, AgentIdentityFailure> {
        if !is_canonical(spelling) {
            return Err(AgentIdentityFailure::NotCanonical);
        }
        Ok(Self { spelling: spelling.to_owned() })
    }

    /// Returns this subscription's spelling.
    #[must_use]
    pub fn as_text(&self) -> &str {
        &self.spelling
    }
}

/// What one operation is called at the agent.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AgentOperationIdentifier {
    /// The digest, in lowercase hexadecimal.
    spelling: String,
}

impl AgentOperationIdentifier {
    /// Returns what one local operation is called at the agent.
    ///
    /// Derived from the local identifier and the partition rather than
    /// allocated by the agent, so a daemon that crashed between submitting and
    /// recording knows what to ask about when it comes back.
    #[must_use]
    pub fn derive(
        author_target_identity_digest: &str,
        selected_environment_revision: &str,
        operation_identifier: &str,
        generation: AgentEventStoreGeneration,
    ) -> Self {
        let mut hasher = Sha256::new();
        absorb(&mut hasher, OPERATION_IDENTIFIER_VERSION.as_bytes());
        absorb(&mut hasher, author_target_identity_digest.as_bytes());
        absorb(&mut hasher, selected_environment_revision.as_bytes());
        absorb(&mut hasher, operation_identifier.as_bytes());
        absorb(&mut hasher, &generation.value().to_be_bytes());
        Self { spelling: render(&hasher.finalize()) }
    }

    /// Returns the operation `spelling` names.
    ///
    /// # Errors
    ///
    /// Returns [`AgentIdentityFailure::NotCanonical`].
    pub fn parse(spelling: &str) -> Result<Self, AgentIdentityFailure> {
        if !is_canonical(spelling) {
            return Err(AgentIdentityFailure::NotCanonical);
        }
        Ok(Self { spelling: spelling.to_owned() })
    }

    /// Returns this operation's spelling.
    #[must_use]
    pub fn as_text(&self) -> &str {
        &self.spelling
    }
}
