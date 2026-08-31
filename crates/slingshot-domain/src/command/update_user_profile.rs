//! Writing a user's profile.
//!
//! A profile is ordinary properties on an ordinary resource, and what makes this
//! its own command is the address: it is derived from the identifier by the
//! author rather than named by the caller, so the caller does not have to know
//! where a particular version of the author puts its users.
//!
//! The property documents follow the shared rule every update in this registry
//! follows, and a group identifier is refused rather than quietly writing a
//! group's node as if it were a profile.

use serde::{Deserialize, Serialize};

use crate::command::authorizable_identity::AuthorizableIdentifier;
use crate::command::create_page::MutationProperties;
use crate::command::repository_path::RepositoryPath;
use crate::command::resource_mutation::{
    MutationResultFailure, PropertyMutationFailure, RemovedPropertyNames, require_property_mutation,
};

/// One request to change a user's profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateUserProfileCommand {
    /// User whose profile is changed.
    pub authorizable_identifier: AuthorizableIdentifier,
    /// Properties to assign to it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub properties: Option<MutationProperties>,
    /// Properties to remove from it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub removed_property_names: Option<RemovedPropertyNames>,
}

impl UpdateUserProfileCommand {
    /// Requires this request to change exactly one thing per property.
    ///
    /// # Errors
    ///
    /// Returns [`PropertyMutationFailure::BothAssignedAndRemoved`] when one
    /// property is named in both documents, and
    /// [`PropertyMutationFailure::ChangesNothing`] when the request would change
    /// nothing at all.
    pub fn require_usable(&self) -> Result<(), PropertyMutationFailure> {
        require_property_mutation(
            self.properties.as_ref(),
            self.removed_property_names.as_ref(),
            false,
        )
    }
}

/// Why a profile was not changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateUserProfileFailure {
    /// Nothing answers to that identifier.
    AuthorizableNotFound,
    /// Something answers to it and it is a group.
    AuthorizableKindMismatch,
    /// This caller may not change it.
    AuthorizableAccessDenied,
    /// A property could not be applied.
    PropertyRejected,
    /// A property named for removal is one the repository keeps.
    PropertyNotRemovable,
    /// The save failed, provably without committing.
    RepositoryCommitFailed,
    /// Nobody can tell whether the save committed.
    MutationOutcomeUnknown,
}

/// One refused profile update.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateUserProfileRefusal {
    /// User this request named.
    pub authorizable_identifier: AuthorizableIdentifier,
    /// Why it was refused.
    pub failure: UpdateUserProfileFailure,
}

impl UpdateUserProfileRefusal {
    /// Returns whether this refusal proves the operation changed nothing.
    #[must_use]
    pub fn proves_no_effect(&self) -> bool {
        !matches!(self.failure, UpdateUserProfileFailure::MutationOutcomeUnknown)
    }

    /// Requires this refusal to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::NotThisRequest`] when it names another
    /// request's user.
    pub fn require_answers(
        &self,
        command: &UpdateUserProfileCommand,
    ) -> Result<(), MutationResultFailure> {
        if self.authorizable_identifier == command.authorizable_identifier {
            Ok(())
        } else {
            Err(MutationResultFailure::NotThisRequest)
        }
    }
}

/// What a completed profile update changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateUserProfileResult {
    /// User whose profile was changed.
    pub authorizable_identifier: AuthorizableIdentifier,
    /// The profile resource the author wrote to.
    pub repository_path: RepositoryPath,
}

impl UpdateUserProfileResult {
    /// Requires this result to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::NotThisRequest`] when it names another
    /// request's user.
    pub fn require_answers(
        &self,
        command: &UpdateUserProfileCommand,
    ) -> Result<(), MutationResultFailure> {
        if self.authorizable_identifier == command.authorizable_identifier {
            Ok(())
        } else {
            Err(MutationResultFailure::NotThisRequest)
        }
    }
}
