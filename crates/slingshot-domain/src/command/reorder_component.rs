//! Moving a component within the parent that holds it.
//!
//! `add_component` appends last and says so. That is a defensible default and a
//! poor only option: where a component sits on a page is what the page looks
//! like, and a surface that can only append can only build a page in one order.
//!
//! # Placement is a closed choice, not a nullable name
//!
//! `before` carries the sibling it goes in front of; `last` carries nothing. The
//! alternative - a nullable sibling name where absence means last - makes an
//! omitted field and an intended value indistinguishable in a document that is
//! missing one by accident.
//!
//! A component cannot precede itself, so a `before` placement naming the
//! component's own name is refused rather than reduced to a move that does
//! nothing.

use serde::{Deserialize, Serialize};

use crate::command::repository_path::{ComponentName, RepositoryPath};
use crate::command::resource_mutation::MutationResultFailure;

/// Where a component is placed among its siblings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum ComponentPlacement {
    /// Directly in front of one named sibling.
    Before {
        /// Sibling the component goes in front of.
        sibling_name: ComponentName,
    },
    /// After every sibling.
    Last,
}

/// One request to reorder a component.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReorderComponentCommand {
    /// Component resource to move.
    pub component_path: RepositoryPath,
    /// Where it goes.
    pub placement: ComponentPlacement,
}

impl ReorderComponentCommand {
    /// Returns the component's own name, when its address has one.
    ///
    /// The repository root has no name, and a component is never at the root, so
    /// an absent name here is an address that could not be a component at all.
    #[must_use]
    pub fn component_name(&self) -> Option<String> {
        self.component_path.segments().last().map(|segment| segment.name().as_text().to_owned())
    }

    /// Requires the placement to name a sibling that is not the component.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::NotThisRequest`] when the placement
    /// names the component itself, which is a move nothing could carry out.
    pub fn require_usable(&self) -> Result<(), MutationResultFailure> {
        match &self.placement {
            ComponentPlacement::Before { sibling_name }
                if self.component_name().as_deref() == Some(sibling_name.as_text()) =>
            {
                Err(MutationResultFailure::NotThisRequest)
            }
            _ => Ok(()),
        }
    }
}

/// Why a component was not reordered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReorderComponentFailure {
    /// Nothing is at the address.
    ComponentNotFound,
    /// Something is there and this caller may not move it.
    ComponentAccessDenied,
    /// The parent does not keep its children in an order.
    ParentNotOrderable,
    /// The named sibling is not under that parent.
    SiblingNotFound,
    /// The save failed, provably without committing.
    RepositoryCommitFailed,
    /// Nobody can tell whether the save committed.
    MutationOutcomeUnknown,
}

/// One refused component reordering.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReorderComponentRefusal {
    /// Component this request named.
    pub component_path: RepositoryPath,
    /// Why it was refused.
    pub failure: ReorderComponentFailure,
}

impl ReorderComponentRefusal {
    /// Returns whether this refusal proves the operation changed nothing.
    #[must_use]
    pub fn proves_no_effect(&self) -> bool {
        !matches!(self.failure, ReorderComponentFailure::MutationOutcomeUnknown)
    }

    /// Requires this refusal to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::NotThisRequest`] when it names another
    /// request's component, and when it reports a missing sibling a request that
    /// named none could not have looked for.
    pub fn require_answers(
        &self,
        command: &ReorderComponentCommand,
    ) -> Result<(), MutationResultFailure> {
        let sought = matches!(self.failure, ReorderComponentFailure::SiblingNotFound);
        let names_a_sibling = matches!(command.placement, ComponentPlacement::Before { .. });
        if self.component_path != command.component_path || (sought && !names_a_sibling) {
            return Err(MutationResultFailure::NotThisRequest);
        }
        Ok(())
    }
}

/// What a completed reordering left behind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReorderComponentResult {
    /// Sibling the component now follows, absent when it is now first.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preceding_sibling_name: Option<ComponentName>,
    /// Component that was moved.
    pub repository_path: RepositoryPath,
}

impl ReorderComponentResult {
    /// Requires this result to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::NotThisRequest`] when it names another
    /// request's component, or when it reports the component as following
    /// itself.
    pub fn require_answers(
        &self,
        command: &ReorderComponentCommand,
    ) -> Result<(), MutationResultFailure> {
        let follows_itself = self
            .preceding_sibling_name
            .as_ref()
            .is_some_and(|name| command.component_name().as_deref() == Some(name.as_text()));
        if self.repository_path != command.component_path || follows_itself {
            return Err(MutationResultFailure::NotThisRequest);
        }
        Ok(())
    }
}
