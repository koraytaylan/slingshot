//! Users and groups, addressed the way an author addresses them.
//!
//! An authorizable is not a repository path. Where a user lives under the
//! authorizable root is the author's answer - it hashes the identifier, and two
//! deployments put the same user in different places - so a command that took a
//! path would be asking the caller to know something it cannot know and would
//! break the moment the author reorganized. What a caller does know is the
//! identifier the authorizable was created under, which is what every command
//! in the family takes.
//!
//! The intermediate path is the one place a caller may say where something
//! goes, and only at creation. It is relative, because it names a position
//! under a root this contract deliberately does not spell: naming that root
//! here would be a second declaration of something the author owns.

use serde::{Deserialize, Serialize};

use crate::command::command_identity::CommandContract;
use crate::command::repository_path::{PathFailure, RepositoryName, accept_within, address_value};

/// Separator between two intermediate-path segments.
const SEGMENT_SEPARATOR: char = '/';

/// The two dot forms a segment may never be.
const DOT_FORMS: &[&str] = &[".", ".."];

/// How many kinds an authorizable can be.
pub const AUTHORIZABLE_KIND_COUNT: usize = 2;

address_value!(
    /// The identifier one user or group is addressed by.
    AuthorizableIdentifier,
    "authorizable identifier"
);

address_value!(
    /// Where under the authorizable root a creation asks for its subject.
    AuthorizableIntermediatePath,
    "authorizable intermediate path"
);

impl AuthorizableIdentifier {
    /// Validates one authorizable identifier.
    ///
    /// # Errors
    ///
    /// Returns [`PathFailure`] when the identifier is empty, longer than the
    /// contract allows, not already in normalization form C, carries a
    /// separator or a control, is one of the two dot forms, or has a leading or
    /// trailing ASCII space.
    pub fn parse(identifier: &str) -> Result<Self, PathFailure> {
        let bound = CommandContract::embedded().limit("maximum_authorizable_identifier_bytes");
        accept_within(identifier, bound, Self::role(), "bytes")?;
        accept_identifier_body(identifier, Self::role())?;
        Ok(Self::from_accepted(identifier))
    }
}

impl AuthorizableIntermediatePath {
    /// Validates one intermediate path.
    ///
    /// # Errors
    ///
    /// Returns [`PathFailure`] when the path is empty, longer than the contract
    /// allows, not already in normalization form C, absolute, ends with a
    /// separator, or carries a segment that is not a repository name.
    pub fn parse(path: &str) -> Result<Self, PathFailure> {
        let bound =
            CommandContract::embedded().limit("maximum_authorizable_intermediate_path_bytes");
        accept_within(path, bound, Self::role(), "bytes")?;
        let refuse = || PathFailure::at(Self::role(), "segment");
        if path.starts_with(SEGMENT_SEPARATOR) || path.ends_with(SEGMENT_SEPARATOR) {
            return Err(refuse());
        }
        for segment in path.split(SEGMENT_SEPARATOR) {
            if DOT_FORMS.contains(&segment) {
                return Err(refuse());
            }
            RepositoryName::parse(segment).map_err(|_| refuse())?;
        }
        Ok(Self::from_accepted(path))
    }

    /// Returns the segments this path names, in order.
    #[must_use]
    pub fn segments(&self) -> Vec<&str> {
        self.as_text().split(SEGMENT_SEPARATOR).collect()
    }
}

/// Which kind of thing an authorizable is.
///
/// Closed, because every command that guards on a kind guards on exactly these
/// two, and a third spelling would be a caller believing an author has a kind
/// this contract can address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorizableKind {
    /// A group, which holds members.
    Group,
    /// A user, which does not.
    User,
}

impl AuthorizableKind {
    /// Returns the wire spelling of this kind.
    #[must_use]
    pub fn as_text(self) -> &'static str {
        match self {
            Self::Group => "group",
            Self::User => "user",
        }
    }

    /// Returns both kinds, in the order they are written.
    #[must_use]
    pub fn both() -> [Self; AUTHORIZABLE_KIND_COUNT] {
        [Self::Group, Self::User]
    }
}

impl ::core::fmt::Display for AuthorizableKind {
    fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        formatter.write_str(self.as_text())
    }
}

/// Requires one identifier body to be addressable.
fn accept_identifier_body(identifier: &str, role: &'static str) -> Result<(), PathFailure> {
    let refuse = |field| PathFailure::at(role, field);
    if DOT_FORMS.contains(&identifier) {
        return Err(refuse("dot form"));
    }
    if identifier.starts_with(' ') || identifier.ends_with(' ') {
        return Err(refuse("space"));
    }
    let usable = identifier
        .chars()
        .all(|character| character != SEGMENT_SEPARATOR && !character.is_control());
    if usable { Ok(()) } else { Err(refuse("character")) }
}
