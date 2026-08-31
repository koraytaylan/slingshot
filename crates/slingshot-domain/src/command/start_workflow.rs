//! Putting content into a workflow.
//!
//! A workflow is how content actually moves through review and publication, and
//! nothing in the registry could start one. This command does: one model, one
//! payload, and the metadata a particular model needs.
//!
//! # The instance identifier is the author's to mint
//!
//! The result carries an instance identifier this contract cannot predict and
//! does not check. What it does check is that the model is the one the request
//! named: a result about another model is a result about another request,
//! whatever instance it carries.
//!
//! # Metadata is bounded text and nothing else
//!
//! Models read metadata as strings. Allowing anything richer would mean this
//! contract deciding how a value serializes into a workflow's data map, which is
//! the author's decision and differs by model.

use serde::de::Error as _;
use serde::{Deserialize, Serialize};

use crate::command::command_identity::CommandContract;
use crate::command::find_pages_containing_phrase::PageTitle;
use crate::command::operational_listing::ListingResultFailure;
use crate::command::process_identity::{
    WorkflowInstanceIdentifier, WorkflowInstanceState, WorkflowModelIdentifier,
};
use crate::command::repository_path::{PathFailure, RepositoryPath, accept_within, address_value};
use crate::command::resource_mutation::MutationResultFailure;

address_value!(
    /// One key of the metadata a model reads.
    WorkflowMetadataKey,
    "workflow metadata key"
);

impl WorkflowMetadataKey {
    /// Validates one metadata key.
    ///
    /// # Errors
    ///
    /// Returns [`PathFailure`] when the key is empty, longer than the contract
    /// allows, not already in normalization form C, carries a control, or has a
    /// leading or trailing ASCII space.
    pub fn parse(key: &str) -> Result<Self, PathFailure> {
        let bound = CommandContract::embedded().limit("maximum_property_name_bytes");
        accept_within(key, bound, Self::role(), "bytes")?;
        let refuse = |field| PathFailure::at(Self::role(), field);
        if key.starts_with(' ') || key.ends_with(' ') {
            return Err(refuse("space"));
        }
        if key.chars().any(char::is_control) {
            return Err(refuse("character"));
        }
        Ok(Self::from_accepted(key))
    }
}

/// The metadata one request hands the model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct WorkflowMetadata {
    /// The entries, by key.
    entries: std::collections::BTreeMap<WorkflowMetadataKey, String>,
}

impl WorkflowMetadata {
    /// Returns the metadata `entries` describes.
    ///
    /// # Errors
    ///
    /// Returns [`ListingResultFailure::TooManyRequested`] above the contract's
    /// entry bound, and [`ListingResultFailure::NotAscendingDistinct`] when a
    /// value is longer than one property string may be.
    pub fn new(
        entries: std::collections::BTreeMap<WorkflowMetadataKey, String>,
    ) -> Result<Self, ListingResultFailure> {
        let contract = CommandContract::embedded();
        if u64::try_from(entries.len()).unwrap_or(u64::MAX)
            > contract.limit("maximum_workflow_metadata_entries")
        {
            return Err(ListingResultFailure::TooManyRequested);
        }
        let value_bound = contract.limit("maximum_property_string_bytes");
        let bounded = entries
            .values()
            .all(|value| u64::try_from(value.len()).unwrap_or(u64::MAX) <= value_bound);
        if bounded { Ok(Self { entries }) } else { Err(ListingResultFailure::NotAscendingDistinct) }
    }

    /// Returns the entries, by key.
    #[must_use]
    pub fn entries(&self) -> &std::collections::BTreeMap<WorkflowMetadataKey, String> {
        &self.entries
    }
}

impl<'de> Deserialize<'de> for WorkflowMetadata {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        let entries = std::collections::BTreeMap::deserialize(deserializer)?;
        Self::new(entries).map_err(Source::Error::custom)
    }
}

/// One request to start a workflow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StartWorkflowCommand {
    /// A note recorded on the instance, when the caller wrote one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    /// The metadata the model reads.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<WorkflowMetadata>,
    /// Model to start.
    pub model_identifier: WorkflowModelIdentifier,
    /// Content the workflow runs on.
    pub payload_path: RepositoryPath,
    /// Title recorded on the instance, when the caller wrote one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<PageTitle>,
}

impl StartWorkflowCommand {
    /// Requires the comment to be within the contract's bound.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::CountTooLarge`] when the comment is
    /// longer than the contract allows.
    pub fn require_usable(&self) -> Result<(), MutationResultFailure> {
        let bound = CommandContract::embedded().limit("maximum_workflow_comment_bytes");
        let within = self
            .comment
            .as_ref()
            .is_none_or(|comment| u64::try_from(comment.len()).unwrap_or(u64::MAX) <= bound);
        if within { Ok(()) } else { Err(MutationResultFailure::CountTooLarge) }
    }
}

/// Why a workflow was not started.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StartWorkflowFailure {
    /// No model answers to that identifier.
    ModelNotFound,
    /// The model is there and cannot be started.
    ModelInvalid,
    /// Nothing is at the payload address.
    PayloadNotFound,
    /// Something is there and this caller may not run a workflow on it.
    PayloadAccessDenied,
    /// The metadata is not something this model accepts.
    MetadataRejected,
    /// The author refused to start it.
    PlatformControlRejected,
    /// Nobody can tell whether it started.
    PlatformControlOutcomeUnknown,
}

/// One refused workflow start.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StartWorkflowRefusal {
    /// Why it was refused.
    pub failure: StartWorkflowFailure,
    /// Model this request named.
    pub model_identifier: WorkflowModelIdentifier,
}

impl StartWorkflowRefusal {
    /// Returns whether this refusal proves the operation changed nothing.
    #[must_use]
    pub fn proves_no_effect(&self) -> bool {
        !matches!(self.failure, StartWorkflowFailure::PlatformControlOutcomeUnknown)
    }

    /// Requires this refusal to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::NotThisRequest`] when it names another
    /// request's model.
    pub fn require_answers(
        &self,
        command: &StartWorkflowCommand,
    ) -> Result<(), MutationResultFailure> {
        if self.model_identifier == command.model_identifier {
            Ok(())
        } else {
            Err(MutationResultFailure::NotThisRequest)
        }
    }
}

/// What starting a workflow produced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StartWorkflowResult {
    /// The instance the author minted.
    pub instance_identifier: WorkflowInstanceIdentifier,
    /// Model it runs.
    pub model_identifier: WorkflowModelIdentifier,
    /// The state it was in when it was reported.
    pub state: WorkflowInstanceState,
}

impl StartWorkflowResult {
    /// Requires this result to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::NotThisRequest`] when it names another
    /// request's model. The instance identifier is the author's to mint and is
    /// therefore not compared against anything.
    pub fn require_answers(
        &self,
        command: &StartWorkflowCommand,
    ) -> Result<(), MutationResultFailure> {
        if self.model_identifier == command.model_identifier {
            Ok(())
        } else {
            Err(MutationResultFailure::NotThisRequest)
        }
    }
}
