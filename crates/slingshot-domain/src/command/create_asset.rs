//! Putting a binary into the repository, and the bound that makes it safe to.
//!
//! This is the only command in the registry that carries content inward, and
//! the interesting part of it is what it refuses. The bytes travel inline, in
//! the request, bounded twice: once by their encoded length before anything is
//! decoded, and once by their decoded length afterwards. An asset larger than
//! that is refused, and the refusal is the honest answer - the alternative is an
//! inbound staging protocol, and a command contract is the wrong place to invent
//! one.
//!
//! The result reports the original rendition's length so a caller can tell what
//! actually landed. That length is required to be the decoded payload's length,
//! which is the one fact the request determines about the bytes and therefore
//! the one an author cannot restate differently.

use serde::{Deserialize, Serialize};

use crate::command::create_page::MutationProperties;
use crate::command::find_assets_by_metadata::AssetByteLength;
use crate::command::repository_path::{PathFailure, RepositoryName, RepositoryPath};
use crate::command::resource_mutation::{InlineBinaryPayload, MutationResultFailure};

/// Exact primary type this command creates.
pub const ASSET_PRIMARY_NODE_TYPE: &str = "dam:Asset";

/// One request to create an asset from bytes the request carries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateAssetCommand {
    /// Metadata to record on the new asset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<MutationProperties>,
    /// Name of the asset to create.
    pub name: RepositoryName,
    /// Node to create it under.
    pub parent_path: RepositoryPath,
    /// The bytes themselves.
    pub payload: InlineBinaryPayload,
}

impl CreateAssetCommand {
    /// Returns where this command would create its asset.
    ///
    /// # Errors
    ///
    /// Returns the path failure when the parent cannot take this child.
    pub fn target_path(&self) -> Result<RepositoryPath, PathFailure> {
        self.parent_path.creatable_child(&self.name)
    }

    /// Returns how many bytes the payload decodes to.
    #[must_use]
    pub fn payload_byte_length(&self) -> u64 {
        self.payload.decoded_byte_length()
    }
}

/// Why an asset was not created.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CreateAssetFailure {
    /// The parent is not there.
    ParentNotFound,
    /// The parent is there and unwritable.
    ParentAccessDenied,
    /// Something is already at the target.
    TargetAlreadyExists,
    /// The bytes are not something this author will store.
    PayloadRejected,
    /// The author refused the payload as larger than it will accept.
    PayloadTooLarge,
    /// The author does not handle assets of this media type.
    MediaTypeUnsupported,
    /// The save failed, provably without committing.
    RepositoryCommitFailed,
    /// Nobody can tell whether the save committed.
    MutationOutcomeUnknown,
}

/// One refused asset creation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateAssetRefusal {
    /// Why it was refused.
    pub failure: CreateAssetFailure,
    /// Target this command computed.
    pub target_path: RepositoryPath,
}

impl CreateAssetRefusal {
    /// Returns whether this refusal proves the operation changed nothing.
    #[must_use]
    pub fn proves_no_effect(&self) -> bool {
        !matches!(self.failure, CreateAssetFailure::MutationOutcomeUnknown)
    }

    /// Requires this refusal to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::NotThisRequest`] when the named target
    /// is not the one the command computes.
    pub fn require_answers(
        &self,
        command: &CreateAssetCommand,
    ) -> Result<(), MutationResultFailure> {
        let expected = command.target_path().map_err(|_| MutationResultFailure::NotThisRequest)?;
        if self.target_path == expected {
            Ok(())
        } else {
            Err(MutationResultFailure::NotThisRequest)
        }
    }
}

/// What a completed asset creation produced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateAssetResult {
    /// Length of the original rendition that was stored.
    pub original_rendition_byte_length: AssetByteLength,
    /// Asset that was created.
    pub repository_path: RepositoryPath,
}

impl CreateAssetResult {
    /// Requires this result to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::NotThisRequest`] when it names an
    /// address this request did not compute, or reports a length that is not the
    /// decoded payload's own.
    pub fn require_answers(
        &self,
        command: &CreateAssetCommand,
    ) -> Result<(), MutationResultFailure> {
        let expected = command.target_path().map_err(|_| MutationResultFailure::NotThisRequest)?;
        if self.repository_path != expected
            || self.original_rendition_byte_length.count() != command.payload_byte_length()
        {
            return Err(MutationResultFailure::NotThisRequest);
        }
        Ok(())
    }
}
