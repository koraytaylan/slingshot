//! Removing an asset.
//!
//! An asset is the thing most likely to be referred to from somewhere its own
//! address gives no hint of - a page in another site, a fragment, a template -
//! which is exactly why this command states a reference policy rather than
//! assuming one, and why an absent asset is a failure rather than a success with
//! nothing to do.

use serde::{Deserialize, Serialize};

use crate::command::repository_path::RepositoryPath;
use crate::command::resource_mutation::{
    DeletedResourceResult, MutationResultFailure, ReferencePolicy,
};

/// One request to remove an asset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeleteAssetCommand {
    /// Asset to remove, with everything below it.
    pub asset_path: RepositoryPath,
    /// What to do about whatever points at it.
    pub reference_policy: ReferencePolicy,
}

impl DeleteAssetCommand {
    /// Reports whether this request refuses to remove a referenced asset.
    #[must_use]
    pub fn refuses_when_referenced(&self) -> bool {
        self.reference_policy == ReferencePolicy::RefuseWhenReferenced
    }
}

/// Why an asset was not removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeleteAssetFailure {
    /// Nothing is at the address.
    AssetNotFound,
    /// Something is there and this caller may not remove it.
    AssetAccessDenied,
    /// Something is there and it is not an asset.
    AssetInvalid,
    /// Something still points at it, and the request said to refuse.
    AssetIsReferenced,
    /// The subtree is larger than the contract permits removing at once.
    DeletionBudgetExceeded,
    /// The save failed, provably without committing.
    RepositoryCommitFailed,
    /// Nobody can tell whether the save committed.
    MutationOutcomeUnknown,
}

/// One refused asset deletion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeleteAssetRefusal {
    /// Asset this request named.
    pub asset_path: RepositoryPath,
    /// Why it was refused.
    pub failure: DeleteAssetFailure,
}

impl DeleteAssetRefusal {
    /// Returns whether this refusal proves the operation changed nothing.
    #[must_use]
    pub fn proves_no_effect(&self) -> bool {
        !matches!(self.failure, DeleteAssetFailure::MutationOutcomeUnknown)
    }

    /// Requires this refusal to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::NotThisRequest`] when it names another
    /// request's asset, and when it reports a reference the request said to
    /// ignore.
    pub fn require_answers(
        &self,
        command: &DeleteAssetCommand,
    ) -> Result<(), MutationResultFailure> {
        let referenced = matches!(self.failure, DeleteAssetFailure::AssetIsReferenced);
        if self.asset_path != command.asset_path
            || (referenced && !command.refuses_when_referenced())
        {
            return Err(MutationResultFailure::NotThisRequest);
        }
        Ok(())
    }
}

/// What a completed asset deletion removed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DeleteAssetResult {
    /// Address that is no longer there, and how much went with it.
    pub deleted: DeletedResourceResult,
}

impl DeleteAssetResult {
    /// Requires this result to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::NotThisRequest`] when it names another
    /// request's asset.
    pub fn require_answers(
        &self,
        command: &DeleteAssetCommand,
    ) -> Result<(), MutationResultFailure> {
        self.deleted.require_answers(&command.asset_path)
    }
}
