//! Removing a user or a group, with a guard against removing the wrong one.
//!
//! This is the mistake with the worst recovery in the family: an identifier that
//! is one character off names something else that exists, and removing it looks
//! exactly like success. So the request states which kind it means to remove and
//! a mismatch is refused - a guard as an argument rather than as a convention
//! somebody follows when they remember to.
//!
//! A group that still has members is refused too. Emptying a group is a separate
//! deliberate act, and doing it as a side effect of a deletion would remove
//! memberships nobody asked about.

use serde::{Deserialize, Serialize};

use crate::command::authorizable_identity::{AuthorizableIdentifier, AuthorizableKind};
use crate::command::repository_path::RepositoryPath;
use crate::command::resource_mutation::MutationResultFailure;

/// One request to remove an authorizable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeleteAuthorizableCommand {
    /// Authorizable to remove.
    pub authorizable_identifier: AuthorizableIdentifier,
    /// The kind this request means to remove.
    pub expected_kind: AuthorizableKind,
}

/// Why an authorizable was not removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeleteAuthorizableFailure {
    /// Nothing answers to that identifier.
    AuthorizableNotFound,
    /// Something answers to it and it is the other kind.
    AuthorizableKindMismatch,
    /// This caller may not remove it.
    AuthorizableAccessDenied,
    /// It is a group and still holds members.
    GroupHasMembers,
    /// The save failed, provably without committing.
    RepositoryCommitFailed,
    /// Nobody can tell whether the save committed.
    MutationOutcomeUnknown,
}

/// One refused authorizable removal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeleteAuthorizableRefusal {
    /// Authorizable this request named.
    pub authorizable_identifier: AuthorizableIdentifier,
    /// Why it was refused.
    pub failure: DeleteAuthorizableFailure,
}

impl DeleteAuthorizableRefusal {
    /// Returns whether this refusal proves the operation changed nothing.
    #[must_use]
    pub fn proves_no_effect(&self) -> bool {
        !matches!(self.failure, DeleteAuthorizableFailure::MutationOutcomeUnknown)
    }

    /// Requires this refusal to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::NotThisRequest`] when it names another
    /// request's authorizable, and when it reports members for a request that
    /// expected a user.
    pub fn require_answers(
        &self,
        command: &DeleteAuthorizableCommand,
    ) -> Result<(), MutationResultFailure> {
        let members = matches!(self.failure, DeleteAuthorizableFailure::GroupHasMembers);
        if self.authorizable_identifier != command.authorizable_identifier
            || (members && command.expected_kind != AuthorizableKind::Group)
        {
            return Err(MutationResultFailure::NotThisRequest);
        }
        Ok(())
    }
}

/// What a completed authorizable removal removed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeleteAuthorizableResult {
    /// Authorizable that is no longer there.
    pub authorizable_identifier: AuthorizableIdentifier,
    /// What kind it was.
    pub kind: AuthorizableKind,
    /// Where it had been.
    pub repository_path: RepositoryPath,
}

impl DeleteAuthorizableResult {
    /// Requires this result to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::NotThisRequest`] when it names another
    /// request's authorizable, or reports a kind other than the expected one -
    /// which would mean the guard did not hold.
    pub fn require_answers(
        &self,
        command: &DeleteAuthorizableCommand,
    ) -> Result<(), MutationResultFailure> {
        if self.authorizable_identifier == command.authorizable_identifier
            && self.kind == command.expected_kind
        {
            Ok(())
        } else {
            Err(MutationResultFailure::NotThisRequest)
        }
    }
}
