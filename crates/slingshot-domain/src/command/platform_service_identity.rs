//! Bundles, declarative service components, and replication agents.
//!
//! Each of these is addressed by a name an author already enforces a grammar
//! for, and a caller that sends something outside that grammar should learn so
//! here. A remote parse failure would say less and would say it after the
//! request had already been carried across a network.
//!
//! Every closed state and kind in this leaf declares its variants in the byte
//! order of their wire spellings, so the derived order and the order a caller
//! writes them in are the same order. A requested set is checked against that
//! order rather than sorted into it, because sorting would accept two documents
//! that mean the same thing and serialize differently.

use serde::{Deserialize, Serialize};

use crate::command::command_identity::CommandContract;
use crate::command::operational_listing::requested_states;
use crate::command::repository_path::{
    PathFailure, accept_opaque_body, accept_within, address_value,
};

/// Separator between two symbolic-name tokens.
const TOKEN_SEPARATOR: char = '.';

/// Separator between two version segments.
const VERSION_SEPARATOR: char = '.';

/// Numeric segments every version spells.
const VERSION_NUMBERS: usize = 3;

/// Segments a version spells when it carries a qualifier.
const QUALIFIED_VERSION_SEGMENTS: usize = 4;

/// States a bundle can be observed in.
pub const BUNDLE_STATE_COUNT: usize = 6;

/// States a declarative service component can be observed in.
pub const COMPONENT_STATE_COUNT: usize = 4;

/// Transports a replication agent can be built on.
pub const REPLICATION_TRANSPORT_KIND_COUNT: usize = 4;

/// Actions a replication queue entry can carry.
pub const REPLICATION_ACTION_COUNT: usize = 4;

address_value!(
    /// The symbolic name one bundle is addressed by.
    BundleSymbolicName,
    "bundle symbolic name"
);

address_value!(
    /// The version one bundle reports.
    BundleVersion,
    "bundle version"
);

address_value!(
    /// The name one declarative service component is addressed by.
    DeclarativeServiceComponentName,
    "declarative service component name"
);

address_value!(
    /// The identifier one replication agent is addressed by.
    ReplicationAgentIdentifier,
    "replication agent identifier"
);

address_value!(
    /// The identifier one entry in a replication queue is addressed by.
    ReplicationQueueEntryIdentifier,
    "replication queue entry identifier"
);

impl BundleSymbolicName {
    /// Validates one bundle symbolic name.
    ///
    /// # Errors
    ///
    /// Returns [`PathFailure`] when the name is empty, longer than the contract
    /// allows, not already in normalization form C, or carries a token that is
    /// empty or holds a character outside the token alphabet.
    pub fn parse(name: &str) -> Result<Self, PathFailure> {
        let bound = CommandContract::embedded().limit("maximum_bundle_symbolic_name_bytes");
        accept_within(name, bound, Self::role(), "bytes")?;
        let refuse = || PathFailure::at(Self::role(), "token");
        for token in name.split(TOKEN_SEPARATOR) {
            if token.is_empty() || !token.chars().all(is_token_character) {
                return Err(refuse());
            }
        }
        Ok(Self::from_accepted(name))
    }
}

impl BundleVersion {
    /// Validates one bundle version.
    ///
    /// # Errors
    ///
    /// Returns [`PathFailure`] when the version is empty, longer than the
    /// contract allows, not already in normalization form C, spells fewer than
    /// three or more than four segments, carries a numeric segment that is
    /// empty, non-numeric, or written with a leading zero, or carries a
    /// qualifier outside the token alphabet.
    pub fn parse(version: &str) -> Result<Self, PathFailure> {
        let bound = CommandContract::embedded().limit("maximum_bundle_version_bytes");
        accept_within(version, bound, Self::role(), "bytes")?;
        let segments: Vec<&str> = version.split(VERSION_SEPARATOR).collect();
        if segments.len() < VERSION_NUMBERS || segments.len() > QUALIFIED_VERSION_SEGMENTS {
            return Err(PathFailure::at(Self::role(), "segments"));
        }
        for segment in &segments[..VERSION_NUMBERS] {
            accept_number(segment, Self::role())?;
        }
        if let Some(qualifier) = segments.get(VERSION_NUMBERS)
            && (qualifier.is_empty() || !qualifier.chars().all(is_token_character))
        {
            return Err(PathFailure::at(Self::role(), "qualifier"));
        }
        Ok(Self::from_accepted(version))
    }
}

impl DeclarativeServiceComponentName {
    /// Validates one component name.
    ///
    /// # Errors
    ///
    /// Returns [`PathFailure`] when the name is empty, longer than the contract
    /// allows, not already in normalization form C, carries a control, or has a
    /// leading or trailing ASCII space.
    pub fn parse(name: &str) -> Result<Self, PathFailure> {
        let bound =
            CommandContract::embedded().limit("maximum_declarative_service_component_name_bytes");
        accept_within(name, bound, Self::role(), "bytes")?;
        accept_opaque_body(name, Self::role())?;
        Ok(Self::from_accepted(name))
    }
}

impl ReplicationAgentIdentifier {
    /// Validates one replication agent identifier.
    ///
    /// # Errors
    ///
    /// Returns [`PathFailure`] when the identifier is empty, longer than the
    /// contract allows, not already in normalization form C, carries a control,
    /// or has a leading or trailing ASCII space.
    pub fn parse(identifier: &str) -> Result<Self, PathFailure> {
        let bound = CommandContract::embedded().limit("maximum_replication_agent_identifier_bytes");
        accept_within(identifier, bound, Self::role(), "bytes")?;
        accept_opaque_body(identifier, Self::role())?;
        Ok(Self::from_accepted(identifier))
    }
}

impl ReplicationQueueEntryIdentifier {
    /// Validates one queue entry identifier.
    ///
    /// # Errors
    ///
    /// Returns [`PathFailure`] when the identifier is empty, longer than the
    /// contract allows, not already in normalization form C, carries a control,
    /// or has a leading or trailing ASCII space.
    pub fn parse(identifier: &str) -> Result<Self, PathFailure> {
        let bound =
            CommandContract::embedded().limit("maximum_replication_queue_entry_identifier_bytes");
        accept_within(identifier, bound, Self::role(), "bytes")?;
        accept_opaque_body(identifier, Self::role())?;
        Ok(Self::from_accepted(identifier))
    }
}

/// The lifecycle state one bundle is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BundleState {
    /// Started and running.
    Active,
    /// Present and not yet resolved.
    Installed,
    /// Wired and not yet started.
    Resolved,
    /// Starting, which is not yet active.
    Starting,
    /// Stopping, which is not yet resolved.
    Stopping,
    /// Removed, and still reported until refreshed away.
    Uninstalled,
}

impl BundleState {
    /// Returns every state, in the order they are written.
    #[must_use]
    pub fn every() -> [Self; BUNDLE_STATE_COUNT] {
        [
            Self::Active,
            Self::Installed,
            Self::Resolved,
            Self::Starting,
            Self::Stopping,
            Self::Uninstalled,
        ]
    }
}

/// The state one declarative service component is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComponentState {
    /// Satisfied and instantiated.
    Active,
    /// Switched off, whether by configuration or by hand.
    Disabled,
    /// Every reference is bound and no instance exists yet.
    Satisfied,
    /// At least one reference is missing.
    Unsatisfied,
}

impl ComponentState {
    /// Returns every state, in the order they are written.
    #[must_use]
    pub fn every() -> [Self; COMPONENT_STATE_COUNT] {
        [Self::Active, Self::Disabled, Self::Satisfied, Self::Unsatisfied]
    }
}

/// What kind of transport one replication agent is built on.
///
/// A kind and never an address. An agent's transport address carries the
/// credential it authenticates with, so this contract reports what sort of thing
/// the agent is and never where it points.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplicationTransportKind {
    /// Invalidates a cache rather than carrying content.
    Flush,
    /// Carries content to a publisher.
    Publish,
    /// Carries content back from a publisher.
    Reverse,
    /// Writes content to a filesystem.
    Static,
}

impl ReplicationTransportKind {
    /// Returns every kind, in the order they are written.
    #[must_use]
    pub fn every() -> [Self; REPLICATION_TRANSPORT_KIND_COUNT] {
        [Self::Flush, Self::Publish, Self::Reverse, Self::Static]
    }
}

/// What one replication queue entry asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplicationAction {
    /// Publish the content at the entry's path.
    Activate,
    /// Withdraw the content at the entry's path.
    Deactivate,
    /// Remove the content at the entry's path.
    Delete,
    /// Exercise the transport without carrying content.
    Test,
}

impl ReplicationAction {
    /// Returns every action, in the order they are written.
    #[must_use]
    pub fn every() -> [Self; REPLICATION_ACTION_COUNT] {
        [Self::Activate, Self::Deactivate, Self::Delete, Self::Test]
    }
}

requested_states!(
    /// A nonempty ascending set of bundle states a listing asks about.
    RequestedBundleStates,
    BundleState,
    "maximum_bundle_states"
);

requested_states!(
    /// A nonempty ascending set of component states a listing asks about.
    RequestedComponentStates,
    ComponentState,
    "maximum_component_states"
);

/// Reports whether one character may appear in a token.
fn is_token_character(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '-' || character == '_'
}

/// Requires one version segment to be a minimally spelled number.
fn accept_number(segment: &str, role: &'static str) -> Result<(), PathFailure> {
    let refuse = || PathFailure::at(role, "number");
    if segment.is_empty() || !segment.chars().all(|character| character.is_ascii_digit()) {
        return Err(refuse());
    }
    if segment.len() > "0".len() && segment.starts_with('0') {
        return Err(refuse());
    }
    Ok(())
}
