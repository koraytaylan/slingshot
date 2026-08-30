//! Creating one page, and knowing afterwards whether it was created.
//!
//! The command creates exactly one `cq:Page` at `parent/page_name` from a
//! template, and applies the title and every initial property to that page's
//! `jcr:content` resource rather than to the page node. That distinction is not
//! cosmetic: the page node carries structure and the content resource carries
//! content, and writing content onto the page node produces a page that renders
//! nothing.
//!
//! Omitted properties keep whatever the template gave them. Nothing here clears
//! a property or removes one, because "leave it alone" and "empty it" are
//! different requests and only one of them was made.
//!
//! # Knowing what happened
//!
//! A mutation that a caller cannot resolve is worse than one that failed. So
//! the agent writes an `InFlight` record before it mutates, and the one save
//! creates the target and a private receipt together in the same transaction.
//! Afterwards the three possible worlds are distinguishable:
//!
//! - a matching receipt is proof the save committed, whatever the target looks
//!   like now, because somebody may have edited or deleted it since;
//! - neither receipt nor target after `InFlight` proves the save did not
//!   commit, which is the only case that permits a retry;
//! - a target without its matching receipt, or a receipt that cannot be read,
//!   is unknown - and unknown never authorizes a retry and never claims no
//!   effect.
//!
//! The receipt is agent-private. It is not user content and it is not a command
//! result.

use serde::{Deserialize, Serialize};

use crate::command::command_identity::CommandContract;
use crate::command::property_value::PropertyValue;
use crate::command::repository_path::{PageName, RepositoryName, RepositoryPath};

/// Exact primary type this command creates.
pub const PAGE_PRIMARY_NODE_TYPE: &str = "cq:Page";

/// Child of the new page that content is written to.
pub const PAGE_CONTENT_CHILD: &str = "jcr:content";

/// Property the title is written to, which no initial property may redefine.
pub const PAGE_TITLE_PROPERTY: &str = "jcr:title";

/// Returns the most properties one mutation may carry.
#[must_use]
pub fn maximum_mutation_properties() -> u64 {
    CommandContract::embedded().limit("maximum_mutation_properties")
}

/// Returns the largest canonical success result one mutation may produce.
#[must_use]
pub fn maximum_mutation_success_result_bytes() -> u64 {
    CommandContract::embedded().limit("maximum_mutation_success_result_bytes")
}

/// Reason a mutation value could not be built.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MutationFailure {
    /// A property map carries more entries than the contract allows.
    #[error("a mutation carries at most {maximum} properties", maximum = maximum_mutation_properties())]
    TooManyProperties,
    /// A property map redefines something the command sets itself.
    #[error("an initial property map does not redefine a property the command sets")]
    PropertyReserved,
    /// A result does not answer the command it claims to answer.
    #[error("a mutation result names the target its command computed")]
    NotThisRequest,
}

/// Where one mutation stands.
///
/// Durable, and written before the save rather than after it, which is what
/// makes an interruption resolvable at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MutationCheckpoint {
    /// Nothing has been attempted. A physical retry may start here.
    NotStarted,
    /// A save was attempted and its outcome is not yet recorded.
    InFlight,
    /// The save committed and that is recorded.
    Committed,
}

/// What reconciliation found after an interrupted attempt.
///
/// The three inputs are independent: whether an attempt was recorded, whether
/// the target is there, and whether a matching receipt is there. What follows
/// from them is not obvious, which is why it is written once here rather than
/// re-derived at each call site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReconciliationEvidence {
    /// Whether the target exists now.
    pub target_present: bool,
    /// Whether a receipt matching this operation was read.
    pub matching_receipt: bool,
    /// Whether a receipt was found but could not be matched or read.
    pub conflicting_receipt: bool,
}

/// What an interrupted attempt resolves to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReconciledOutcome {
    /// The save committed. Replay the recorded result; do not mutate again.
    Committed,
    /// The save did not commit. One retry is permitted.
    NotCommitted,
    /// Nobody can tell. No retry, and no claim of no effect.
    Unknown,
    /// Nothing was attempted, and the target was already there beforehand.
    TargetAlreadyExists,
}

impl ReconciliationEvidence {
    /// Returns what this evidence resolves to after `checkpoint`.
    ///
    /// A matching receipt wins over everything, including an absent target: the
    /// target may have been edited, moved, or deleted by somebody else since,
    /// and none of that unmakes the commit. Current target content is never
    /// compared against the receipt to try to disprove it.
    #[must_use]
    pub fn resolve(self, checkpoint: MutationCheckpoint) -> ReconciledOutcome {
        if self.matching_receipt && !self.conflicting_receipt {
            return ReconciledOutcome::Committed;
        }
        if checkpoint == MutationCheckpoint::NotStarted {
            return if self.target_present {
                ReconciledOutcome::TargetAlreadyExists
            } else {
                ReconciledOutcome::NotCommitted
            };
        }
        if self.conflicting_receipt || self.target_present {
            return ReconciledOutcome::Unknown;
        }
        ReconciledOutcome::NotCommitted
    }
}

/// Properties one mutation applies.
///
/// Bounded, and unable to carry a property the command sets itself. A map that
/// could redefine the title would make two parts of one request disagree, and
/// nothing would say which won.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct MutationProperties {
    /// The properties, by name.
    values: std::collections::BTreeMap<String, PropertyValue>,
}

impl MutationProperties {
    /// Returns the properties `values` describe, with `reserved` refused.
    ///
    /// # Errors
    ///
    /// Returns [`MutationFailure::TooManyProperties`] above the named bound and
    /// [`MutationFailure::PropertyReserved`] when a reserved name is present.
    pub fn new(
        values: std::collections::BTreeMap<String, PropertyValue>,
        reserved: &[&str],
    ) -> Result<Self, MutationFailure> {
        if u64::try_from(values.len()).unwrap_or(u64::MAX) > maximum_mutation_properties() {
            return Err(MutationFailure::TooManyProperties);
        }
        if reserved.iter().any(|name| values.contains_key(*name)) {
            return Err(MutationFailure::PropertyReserved);
        }
        Ok(Self { values })
    }

    /// Returns the properties, by name.
    #[must_use]
    pub fn values(&self) -> &std::collections::BTreeMap<String, PropertyValue> {
        &self.values
    }
}

impl<'de> Deserialize<'de> for MutationProperties {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        use serde::de::Error as _;

        let values =
            std::collections::BTreeMap::<String, PropertyValue>::deserialize(deserializer)?;
        Self::new(values, &[]).map_err(Source::Error::custom)
    }
}

/// One request to create a page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatePageCommand {
    /// Properties to apply to the new page's content resource.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_properties: Option<MutationProperties>,
    /// Name of the page to create.
    pub page_name: PageName,
    /// Node to create it under.
    pub parent_path: RepositoryPath,
    /// Template to create it from.
    pub template_path: RepositoryPath,
    /// Title to write to its content resource.
    pub title: String,
}

impl CreatePageCommand {
    /// Returns where this command would create its page.
    ///
    /// Computed rather than supplied, so a failure naming a target names the
    /// one this request would have made and not one a caller asserted.
    ///
    /// # Errors
    ///
    /// Returns the path failure when the parent cannot take this child, which
    /// is the same refusal the path grammar would make.
    pub fn target_path(
        &self,
    ) -> Result<RepositoryPath, crate::command::repository_path::PathFailure> {
        let name = RepositoryName::parse(self.page_name.as_text())?;
        self.parent_path.creatable_child(&name)
    }

    /// Requires the initial properties to leave the title alone.
    ///
    /// # Errors
    ///
    /// Returns [`MutationFailure::PropertyReserved`] when the map carries the
    /// title property this command sets from its own field.
    pub fn require_title_not_redefined(&self) -> Result<(), MutationFailure> {
        let redefined = self
            .initial_properties
            .as_ref()
            .is_some_and(|properties| properties.values().contains_key(PAGE_TITLE_PROPERTY));
        if redefined { Err(MutationFailure::PropertyReserved) } else { Ok(()) }
    }
}

/// Why a page was not created.
///
/// Every category but the last is emitted only with authoritative evidence that
/// this operation changed nothing. The last one says the opposite of nothing:
/// it may have changed something, and it never authorizes a retry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "failure", rename_all = "snake_case", deny_unknown_fields)]
pub enum CreatePageRefusal {
    /// Something is already at the target.
    TargetAlreadyExists {
        /// Target this command computed.
        target_path: RepositoryPath,
    },
    /// The parent is not there.
    ParentNotFound {
        /// Target this command computed.
        target_path: RepositoryPath,
    },
    /// The parent is there and unwritable.
    ParentAccessDenied {
        /// Target this command computed.
        target_path: RepositoryPath,
    },
    /// The template is not there.
    TemplateNotFound {
        /// Target this command computed.
        target_path: RepositoryPath,
    },
    /// The template is there and is not one.
    TemplateInvalid {
        /// Target this command computed.
        target_path: RepositoryPath,
    },
    /// A property could not be applied.
    PropertyRejected {
        /// Target this command computed.
        target_path: RepositoryPath,
    },
    /// The save failed, provably without committing.
    RepositoryCommitFailed {
        /// Target this command computed.
        target_path: RepositoryPath,
    },
    /// Nobody can tell whether the save committed.
    MutationOutcomeUnknown {
        /// Target this command computed.
        target_path: RepositoryPath,
    },
}

impl CreatePageRefusal {
    /// Returns the target this refusal names.
    #[must_use]
    pub fn target_path(&self) -> &RepositoryPath {
        match self {
            Self::TargetAlreadyExists { target_path }
            | Self::ParentNotFound { target_path }
            | Self::ParentAccessDenied { target_path }
            | Self::TemplateNotFound { target_path }
            | Self::TemplateInvalid { target_path }
            | Self::PropertyRejected { target_path }
            | Self::RepositoryCommitFailed { target_path }
            | Self::MutationOutcomeUnknown { target_path } => target_path,
        }
    }

    /// Returns whether this refusal proves the operation changed nothing.
    #[must_use]
    pub fn proves_no_effect(&self) -> bool {
        !matches!(self, Self::MutationOutcomeUnknown { .. })
    }

    /// Requires this refusal to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`MutationFailure::NotThisRequest`] when the named target is not
    /// the one the command computes.
    pub fn require_answers(&self, command: &CreatePageCommand) -> Result<(), MutationFailure> {
        let expected = command.target_path().map_err(|_| MutationFailure::NotThisRequest)?;
        if *self.target_path() == expected { Ok(()) } else { Err(MutationFailure::NotThisRequest) }
    }
}

/// What a completed creation produced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatePageResult {
    /// Page that was created.
    pub target_path: RepositoryPath,
}

impl CreatePageResult {
    /// Requires this result to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`MutationFailure::NotThisRequest`] when the named target is not
    /// the one the command computes.
    pub fn require_answers(&self, command: &CreatePageCommand) -> Result<(), MutationFailure> {
        let expected = command.target_path().map_err(|_| MutationFailure::NotThisRequest)?;
        if self.target_path == expected { Ok(()) } else { Err(MutationFailure::NotThisRequest) }
    }
}
