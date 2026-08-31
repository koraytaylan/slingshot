//! Removing a page and everything under it.
//!
//! This is the command whose mistakes cannot be undone from here, so two of its
//! decisions are deliberately not defaults.
//!
//! # The reference policy is stated
//!
//! Whatever else points at a page keeps pointing at it after the page is gone.
//! Refusing to delete a referenced page is right for an operator tidying up and
//! wrong for one decommissioning a site; ignoring references is the reverse.
//! There is no default, so a caller has decided once, in the request, rather
//! than discovering the decision afterwards.
//!
//! # An absent page is a failure
//!
//! Deleting something that is not there is not success with nothing to do. A
//! caller that meant a different address learns it here, instead of from the
//! content that is still present an hour later. That also makes this command not
//! intrinsically idempotent, so it carries an operation key like every other
//! write in the registry.

use serde::{Deserialize, Serialize};

use crate::command::repository_path::RepositoryPath;
use crate::command::resource_mutation::{
    DeletedResourceResult, MutationResultFailure, ReferencePolicy,
};

/// One request to remove a page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeletePageCommand {
    /// Page to remove, with everything below it.
    pub page_path: RepositoryPath,
    /// What to do about whatever points at it.
    pub reference_policy: ReferencePolicy,
}

impl DeletePageCommand {
    /// Reports whether this request refuses to remove a referenced page.
    #[must_use]
    pub fn refuses_when_referenced(&self) -> bool {
        self.reference_policy == ReferencePolicy::RefuseWhenReferenced
    }
}

/// Why a page was not removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeletePageFailure {
    /// Nothing is at the address.
    TargetNotFound,
    /// Something is there and this caller may not remove it.
    TargetAccessDenied,
    /// Something is there and it is not a page.
    TargetNotAPage,
    /// Something still points at it, and the request said to refuse.
    TargetIsReferenced,
    /// The subtree is larger than the contract permits removing at once.
    DeletionBudgetExceeded,
    /// The save failed, provably without committing.
    RepositoryCommitFailed,
    /// Nobody can tell whether the save committed.
    MutationOutcomeUnknown,
}

/// One refused page deletion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeletePageRefusal {
    /// Why it was refused.
    pub failure: DeletePageFailure,
    /// Page this request named.
    pub page_path: RepositoryPath,
}

impl DeletePageRefusal {
    /// Returns whether this refusal proves the operation changed nothing.
    #[must_use]
    pub fn proves_no_effect(&self) -> bool {
        !matches!(self.failure, DeletePageFailure::MutationOutcomeUnknown)
    }

    /// Requires this refusal to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::NotThisRequest`] when it names another
    /// request's page, and when it reports a reference the request said to
    /// ignore.
    pub fn require_answers(
        &self,
        command: &DeletePageCommand,
    ) -> Result<(), MutationResultFailure> {
        let referenced = matches!(self.failure, DeletePageFailure::TargetIsReferenced);
        if self.page_path != command.page_path || (referenced && !command.refuses_when_referenced())
        {
            return Err(MutationResultFailure::NotThisRequest);
        }
        Ok(())
    }
}

/// What a completed page deletion removed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DeletePageResult {
    /// Address that is no longer there, and how much went with it.
    pub deleted: DeletedResourceResult,
}

impl DeletePageResult {
    /// Requires this result to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::NotThisRequest`] when it names another
    /// request's page.
    pub fn require_answers(
        &self,
        command: &DeletePageCommand,
    ) -> Result<(), MutationResultFailure> {
        self.deleted.require_answers(&command.page_path)
    }
}
