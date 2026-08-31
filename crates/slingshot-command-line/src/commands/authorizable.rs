//! Creating, changing, disabling, removing, and grouping authorizables.
//!
//! No option here carries a credential, and there is no option that could: a
//! created account has no password, and the command's own documentation says so
//! rather than leaving a caller to discover it at the first sign-in.
//!
//! `--authorizable` names the subject everywhere except the two membership
//! commands, which name a `--group` and a `--member` because a membership is
//! about two authorizables and calling either of them the subject would be
//! picking one arbitrarily.

use slingshot_domain::command::authorizable_identity::{
    AuthorizableIdentifier, AuthorizableIntermediatePath, AuthorizableKind,
};
use slingshot_domain::command::catalog::Command;
use slingshot_domain::command::create_authorizable::{CreateGroupCommand, CreateUserCommand};
use slingshot_domain::command::delete_authorizable::DeleteAuthorizableCommand;
use slingshot_domain::command::group_membership::{
    AddGroupMemberCommand, RemoveGroupMemberCommand,
};
use slingshot_domain::command::list_group_members::ListGroupMembersCommand;
use slingshot_domain::command::set_user_disabled::SetUserDisabledCommand;
use slingshot_domain::command::update_user_profile::UpdateUserProfileCommand;

use crate::commands::content::{RequestRefusal, require_key, required};
use crate::commands::operational_values::{
    decision, flag, optional_text, removed_property_names, unusable,
};
use crate::commands::page_mutation::properties;
use crate::commands::path_query::window;
use crate::invocation::{
    AUTHORIZABLE_OPTION, DISABLED_OPTION, EXPECTED_KIND_OPTION, GROUP_OPTION,
    INCLUDE_INDIRECT_OPTION, INTERMEDIATE_PATH_OPTION, Invocation, MEMBER_OPTION, REASON_OPTION,
};

/// The wire name of the user creation.
pub const CREATE_USER: &str = "create_user";

/// The wire name of the group creation.
pub const CREATE_GROUP: &str = "create_group";

/// The wire name of the profile update.
pub const UPDATE_USER_PROFILE: &str = "update_user_profile";

/// The wire name of the disablement.
pub const SET_USER_DISABLED: &str = "set_user_disabled";

/// The wire name of the removal.
pub const DELETE_AUTHORIZABLE: &str = "delete_authorizable";

/// The wire name of the membership addition.
pub const ADD_GROUP_MEMBER: &str = "add_group_member";

/// The wire name of the membership removal.
pub const REMOVE_GROUP_MEMBER: &str = "remove_group_member";

/// The wire name of the member listing.
pub const LIST_GROUP_MEMBERS: &str = "list_group_members";

/// The spelling that disables an account.
pub const DISABLED: &str = "disabled";

/// The spelling that enables one again.
pub const ENABLED: &str = "enabled";

/// Every command this family builds.
const NAMES: &[&str] = &[
    CREATE_USER,
    CREATE_GROUP,
    UPDATE_USER_PROFILE,
    SET_USER_DISABLED,
    DELETE_AUTHORIZABLE,
    ADD_GROUP_MEMBER,
    REMOVE_GROUP_MEMBER,
    LIST_GROUP_MEMBERS,
];

/// Returns the typed request one invocation describes.
///
/// # Errors
///
/// Returns [`RequestRefusal`] naming the first thing that is wrong, or that this
/// family builds no such command.
pub fn build(invocation: &Invocation) -> Result<Command, RequestRefusal> {
    if !NAMES.contains(&invocation.verb.as_str()) {
        return Err(RequestRefusal::AnotherCommand { named: invocation.verb.clone() });
    }
    require_key(invocation)?;
    build_subject(invocation).unwrap_or_else(|| build_membership(invocation))
}

/// Returns the single-subject command one invocation describes, when it is one.
fn build_subject(invocation: &Invocation) -> Option<Result<Command, RequestRefusal>> {
    let built = match invocation.verb.as_str() {
        CREATE_USER => create_user(invocation),
        CREATE_GROUP => create_group(invocation),
        UPDATE_USER_PROFILE => update_profile(invocation),
        SET_USER_DISABLED => set_disabled(invocation),
        DELETE_AUTHORIZABLE => delete(invocation),
        _ => return None,
    };
    Some(built)
}

/// Returns the membership command one invocation describes.
fn build_membership(invocation: &Invocation) -> Result<Command, RequestRefusal> {
    let group_identifier = named(invocation, GROUP_OPTION)?;
    match invocation.verb.as_str() {
        ADD_GROUP_MEMBER => Ok(Command::AddGroupMember(AddGroupMemberCommand {
            group_identifier,
            member_identifier: named(invocation, MEMBER_OPTION)?,
        })),
        REMOVE_GROUP_MEMBER => Ok(Command::RemoveGroupMember(RemoveGroupMemberCommand {
            group_identifier,
            member_identifier: named(invocation, MEMBER_OPTION)?,
        })),
        _ => Ok(Command::ListGroupMembers(ListGroupMembersCommand {
            group_identifier,
            include_indirect: flag(invocation, INCLUDE_INDIRECT_OPTION),
            result_window: window(invocation)?,
        })),
    }
}

/// Returns the user creation one invocation describes.
fn create_user(invocation: &Invocation) -> Result<Command, RequestRefusal> {
    Ok(Command::CreateUser(CreateUserCommand {
        authorizable_identifier: subject(invocation)?,
        intermediate_path: intermediate_path(invocation)?,
        properties: properties(invocation, &[])?,
    }))
}

/// Returns the group creation one invocation describes.
fn create_group(invocation: &Invocation) -> Result<Command, RequestRefusal> {
    Ok(Command::CreateGroup(CreateGroupCommand {
        authorizable_identifier: subject(invocation)?,
        intermediate_path: intermediate_path(invocation)?,
        properties: properties(invocation, &[])?,
    }))
}

/// Returns the profile update one invocation describes.
fn update_profile(invocation: &Invocation) -> Result<Command, RequestRefusal> {
    Ok(Command::UpdateUserProfile(UpdateUserProfileCommand {
        authorizable_identifier: subject(invocation)?,
        properties: properties(invocation, &[])?,
        removed_property_names: removed_property_names(invocation)?,
    }))
}

/// Returns the disablement one invocation describes.
fn set_disabled(invocation: &Invocation) -> Result<Command, RequestRefusal> {
    Ok(Command::SetUserDisabled(SetUserDisabledCommand {
        authorizable_identifier: subject(invocation)?,
        disabled: decision(invocation, DISABLED_OPTION, DISABLED, ENABLED)?,
        reason: optional_text(invocation, REASON_OPTION),
    }))
}

/// Returns the removal one invocation describes.
fn delete(invocation: &Invocation) -> Result<Command, RequestRefusal> {
    let expected_kind: AuthorizableKind =
        serde_json::from_str(&format!("\"{}\"", required(invocation, EXPECTED_KIND_OPTION)?))
            .map_err(|_| unusable(EXPECTED_KIND_OPTION))?;
    Ok(Command::DeleteAuthorizable(DeleteAuthorizableCommand {
        authorizable_identifier: subject(invocation)?,
        expected_kind,
    }))
}

/// Returns the authorizable this invocation acts on.
fn subject(invocation: &Invocation) -> Result<AuthorizableIdentifier, RequestRefusal> {
    named(invocation, AUTHORIZABLE_OPTION)
}

/// Returns the authorizable `option` names.
fn named(invocation: &Invocation, option: &str) -> Result<AuthorizableIdentifier, RequestRefusal> {
    AuthorizableIdentifier::parse(required(invocation, option)?).map_err(|_| unusable(option))
}

/// Returns where under the authorizable root a creation asks for its subject.
fn intermediate_path(
    invocation: &Invocation,
) -> Result<Option<AuthorizableIntermediatePath>, RequestRefusal> {
    optional_text(invocation, INTERMEDIATE_PATH_OPTION)
        .map(|stated| {
            AuthorizableIntermediatePath::parse(&stated)
                .map_err(|_| unusable(INTERMEDIATE_PATH_OPTION))
        })
        .transpose()
}
