//! Disabling an account, and enabling it again.
//!
//! This is the administrative action taken under the most time pressure, so it
//! is one command with one decision rather than two commands whose difference
//! somebody has to remember at the wrong moment.
//!
//! A reason is accepted only when disabling. A reason for an enabling would be a
//! value the author stores and nobody ever reads, and a member that is sometimes
//! meaningless is a member somebody eventually fills in meaninglessly.

use serde::{Deserialize, Serialize};

use crate::command::authorizable_identity::AuthorizableIdentifier;
use crate::command::command_identity::CommandContract;
use crate::command::resource_mutation::MutationResultFailure;

/// One request to disable or enable a user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetUserDisabledCommand {
    /// User to act on.
    pub authorizable_identifier: AuthorizableIdentifier,
    /// Whether the user should be disabled afterwards.
    pub disabled: bool,
    /// Why, when the request disables.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl SetUserDisabledCommand {
    /// Requires the reason to belong to this request and to be within bound.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::NotThisRequest`] when a reason is given
    /// for an enabling, and [`MutationResultFailure::CountTooLarge`] when the
    /// reason is longer than the contract allows.
    pub fn require_usable(&self) -> Result<(), MutationResultFailure> {
        let Some(reason) = self.reason.as_ref() else {
            return Ok(());
        };
        if !self.disabled {
            return Err(MutationResultFailure::NotThisRequest);
        }
        let bound = CommandContract::embedded().limit("maximum_authorizable_disabled_reason_bytes");
        if u64::try_from(reason.len()).unwrap_or(u64::MAX) > bound {
            return Err(MutationResultFailure::CountTooLarge);
        }
        Ok(())
    }
}

/// Why a user was not disabled or enabled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SetUserDisabledFailure {
    /// Nothing answers to that identifier.
    AuthorizableNotFound,
    /// Something answers to it and it is a group.
    AuthorizableKindMismatch,
    /// This caller may not act on it.
    AuthorizableAccessDenied,
    /// The author refused the change.
    PlatformControlRejected,
    /// Nobody can tell whether the change took effect.
    PlatformControlOutcomeUnknown,
}

/// One refused disablement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetUserDisabledRefusal {
    /// User this request named.
    pub authorizable_identifier: AuthorizableIdentifier,
    /// Why it was refused.
    pub failure: SetUserDisabledFailure,
}

impl SetUserDisabledRefusal {
    /// Returns whether this refusal proves the operation changed nothing.
    #[must_use]
    pub fn proves_no_effect(&self) -> bool {
        !matches!(self.failure, SetUserDisabledFailure::PlatformControlOutcomeUnknown)
    }

    /// Requires this refusal to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::NotThisRequest`] when it names another
    /// request's user.
    pub fn require_answers(
        &self,
        command: &SetUserDisabledCommand,
    ) -> Result<(), MutationResultFailure> {
        if self.authorizable_identifier == command.authorizable_identifier {
            Ok(())
        } else {
            Err(MutationResultFailure::NotThisRequest)
        }
    }
}

/// What the author observed after the change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetUserDisabledResult {
    /// User that was acted on.
    pub authorizable_identifier: AuthorizableIdentifier,
    /// Whether the user was disabled afterwards.
    pub disabled: bool,
}

impl SetUserDisabledResult {
    /// Requires this result to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::NotThisRequest`] when it names another
    /// request's user.
    pub fn require_answers(
        &self,
        command: &SetUserDisabledCommand,
    ) -> Result<(), MutationResultFailure> {
        if self.authorizable_identifier == command.authorizable_identifier {
            Ok(())
        } else {
            Err(MutationResultFailure::NotThisRequest)
        }
    }
}
