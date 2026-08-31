//! Creating a user or a group, and the credential this contract will not carry.
//!
//! The two commands differ in one field and share every rule, so they live
//! together: two modules would be two places for the same identifier grammar,
//! the same intermediate-path rule, and the same result to drift apart.
//!
//! # A created user has no password
//!
//! No argument here accepts a password, a key, or a token, and the types make
//! that structural rather than a promise. The consequence is stated plainly
//! because it is a real limitation and a caller should meet it here rather than
//! at the first failed sign-in: an account created by this command cannot
//! authenticate until an administrator supplies a credential through a channel
//! this contract does not provide.
//!
//! # The address is the author's answer
//!
//! Where an authorizable lives under the authorizable root is decided by the
//! author, which hashes the identifier and may reorganize between versions. The
//! result reports the address; the request does not determine it, so nothing
//! here compares it against anything.

use serde::{Deserialize, Serialize};

use crate::command::authorizable_identity::{
    AuthorizableIdentifier, AuthorizableIntermediatePath, AuthorizableKind,
};
use crate::command::create_page::MutationProperties;
use crate::command::repository_path::RepositoryPath;
use crate::command::resource_mutation::MutationResultFailure;

/// One request to create a user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateUserCommand {
    /// Identifier the user is addressed by.
    pub authorizable_identifier: AuthorizableIdentifier,
    /// Where under the authorizable root to put it, when the caller said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intermediate_path: Option<AuthorizableIntermediatePath>,
    /// Profile properties to record on it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub properties: Option<MutationProperties>,
}

/// One request to create a group.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateGroupCommand {
    /// Identifier the group is addressed by.
    pub authorizable_identifier: AuthorizableIdentifier,
    /// Where under the authorizable root to put it, when the caller said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intermediate_path: Option<AuthorizableIntermediatePath>,
    /// Properties to record on it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub properties: Option<MutationProperties>,
}

/// Why an authorizable was not created.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CreateAuthorizableFailure {
    /// Something already answers to that identifier.
    AuthorizableAlreadyExists,
    /// The author refused the identifier.
    IdentifierRejected,
    /// The author refused the intermediate path.
    IntermediatePathRejected,
    /// A property could not be applied.
    PropertyRejected,
    /// This caller may not create authorizables.
    AuthorizableAccessDenied,
    /// The save failed, provably without committing.
    RepositoryCommitFailed,
    /// Nobody can tell whether the save committed.
    MutationOutcomeUnknown,
}

/// One refused authorizable creation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateAuthorizableRefusal {
    /// Identifier this request named.
    pub authorizable_identifier: AuthorizableIdentifier,
    /// Why it was refused.
    pub failure: CreateAuthorizableFailure,
}

impl CreateAuthorizableRefusal {
    /// Returns whether this refusal proves the operation changed nothing.
    #[must_use]
    pub fn proves_no_effect(&self) -> bool {
        !matches!(self.failure, CreateAuthorizableFailure::MutationOutcomeUnknown)
    }

    /// Requires this refusal to name `identifier`.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::NotThisRequest`] when it names another
    /// request's authorizable.
    pub fn require_answers(
        &self,
        identifier: &AuthorizableIdentifier,
    ) -> Result<(), MutationResultFailure> {
        if self.authorizable_identifier == *identifier {
            Ok(())
        } else {
            Err(MutationResultFailure::NotThisRequest)
        }
    }
}

/// What a completed authorizable creation produced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateAuthorizableResult {
    /// Identifier the authorizable answers to.
    pub authorizable_identifier: AuthorizableIdentifier,
    /// What kind of authorizable was created.
    pub kind: AuthorizableKind,
    /// Where the author put it.
    pub repository_path: RepositoryPath,
}

impl CreateAuthorizableResult {
    /// Requires this result to name `identifier` and `kind`.
    ///
    /// The address is not compared, because the request never determined it.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::NotThisRequest`] when it names another
    /// request's authorizable, or reports a kind the request did not ask to
    /// create.
    pub fn require_answers(
        &self,
        identifier: &AuthorizableIdentifier,
        kind: AuthorizableKind,
    ) -> Result<(), MutationResultFailure> {
        if self.authorizable_identifier == *identifier && self.kind == kind {
            Ok(())
        } else {
            Err(MutationResultFailure::NotThisRequest)
        }
    }
}
