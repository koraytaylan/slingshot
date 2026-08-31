//! Adding a member to a group, and taking one out.
//!
//! One relationship changed in two directions, landing together because the
//! question a caller has afterwards is the same in both: did this change
//! anything. Each result answers it - whether the membership already existed,
//! whether it existed at all - so a no-op is distinguishable from a change
//! without a second request.
//!
//! A group cannot contain itself, so a request whose two identifiers are equal
//! is refused here rather than by the author. A membership that would make a
//! group its own ancestor is a cycle the author detects and this contract names.
//!
//! A member may be a group. Groups belong to groups in every deployment that has
//! more than a handful of them, and refusing that would be refusing the ordinary
//! case.

use serde::{Deserialize, Serialize};

use crate::command::authorizable_identity::AuthorizableIdentifier;
use crate::command::resource_mutation::MutationResultFailure;

/// One request to add a member to a group.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddGroupMemberCommand {
    /// Group to add to.
    pub group_identifier: AuthorizableIdentifier,
    /// Authorizable to add.
    pub member_identifier: AuthorizableIdentifier,
}

/// One request to remove a member from a group.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoveGroupMemberCommand {
    /// Group to remove from.
    pub group_identifier: AuthorizableIdentifier,
    /// Authorizable to remove.
    pub member_identifier: AuthorizableIdentifier,
}

impl AddGroupMemberCommand {
    /// Requires the group and the member to be different authorizables.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::NotThisRequest`] when they are the same,
    /// which is a membership nothing could hold.
    pub fn require_usable(&self) -> Result<(), MutationResultFailure> {
        require_distinct(&self.group_identifier, &self.member_identifier)
    }
}

impl RemoveGroupMemberCommand {
    /// Requires the group and the member to be different authorizables.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::NotThisRequest`] when they are the same,
    /// which is a membership nothing could hold.
    pub fn require_usable(&self) -> Result<(), MutationResultFailure> {
        require_distinct(&self.group_identifier, &self.member_identifier)
    }
}

/// Why a membership was not changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupMembershipFailure {
    /// Nothing answers to the group identifier.
    GroupNotFound,
    /// Nothing answers to the member identifier.
    MemberNotFound,
    /// The group identifier names a user.
    AuthorizableKindMismatch,
    /// This caller may not change that membership.
    AuthorizableAccessDenied,
    /// The membership would make a group its own ancestor.
    MembershipCycleRefused,
    /// The save failed, provably without committing.
    RepositoryCommitFailed,
    /// Nobody can tell whether the save committed.
    MutationOutcomeUnknown,
}

/// One refused membership change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroupMembershipRefusal {
    /// Why it was refused.
    pub failure: GroupMembershipFailure,
    /// Group this request named.
    pub group_identifier: AuthorizableIdentifier,
    /// Member this request named.
    pub member_identifier: AuthorizableIdentifier,
}

impl GroupMembershipRefusal {
    /// Returns whether this refusal proves the operation changed nothing.
    #[must_use]
    pub fn proves_no_effect(&self) -> bool {
        !matches!(self.failure, GroupMembershipFailure::MutationOutcomeUnknown)
    }

    /// Requires this refusal to name `group` and `member`.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::NotThisRequest`] when either identifier
    /// is another request's.
    pub fn require_answers(
        &self,
        group: &AuthorizableIdentifier,
        member: &AuthorizableIdentifier,
    ) -> Result<(), MutationResultFailure> {
        if self.group_identifier == *group && self.member_identifier == *member {
            Ok(())
        } else {
            Err(MutationResultFailure::NotThisRequest)
        }
    }
}

/// What adding a member did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddGroupMemberResult {
    /// Whether the membership was already there before this request.
    pub already_a_member: bool,
    /// Group that was added to.
    pub group_identifier: AuthorizableIdentifier,
    /// Authorizable that is now a member.
    pub member_identifier: AuthorizableIdentifier,
}

/// What removing a member did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoveGroupMemberResult {
    /// Group that was removed from.
    pub group_identifier: AuthorizableIdentifier,
    /// Authorizable that is no longer a member.
    pub member_identifier: AuthorizableIdentifier,
    /// Whether the membership existed at all before this request.
    pub was_a_member: bool,
}

impl AddGroupMemberResult {
    /// Requires this result to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::NotThisRequest`] when either identifier
    /// is another request's.
    pub fn require_answers(
        &self,
        command: &AddGroupMemberCommand,
    ) -> Result<(), MutationResultFailure> {
        if self.group_identifier == command.group_identifier
            && self.member_identifier == command.member_identifier
        {
            Ok(())
        } else {
            Err(MutationResultFailure::NotThisRequest)
        }
    }
}

impl RemoveGroupMemberResult {
    /// Requires this result to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::NotThisRequest`] when either identifier
    /// is another request's.
    pub fn require_answers(
        &self,
        command: &RemoveGroupMemberCommand,
    ) -> Result<(), MutationResultFailure> {
        if self.group_identifier == command.group_identifier
            && self.member_identifier == command.member_identifier
        {
            Ok(())
        } else {
            Err(MutationResultFailure::NotThisRequest)
        }
    }
}

/// Requires two identifiers to name different authorizables.
fn require_distinct(
    group: &AuthorizableIdentifier,
    member: &AuthorizableIdentifier,
) -> Result<(), MutationResultFailure> {
    if group == member { Err(MutationResultFailure::NotThisRequest) } else { Ok(()) }
}
