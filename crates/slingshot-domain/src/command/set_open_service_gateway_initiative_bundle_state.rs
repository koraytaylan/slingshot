//! Starting, stopping, or refreshing a bundle.
//!
//! This command changes no content and is plainly not a read, which is the case
//! that made this registry widen what access means: retained state is any state
//! the author still has after the command returns, and a bundle's lifecycle
//! state is exactly that.
//!
//! # The answer is what was observed, not what was asked for
//!
//! A transition can be accepted and not take effect - a bundle that starts and
//! immediately fails its activator, one that stops and is restarted by something
//! else. The result reports the state the author observed afterwards, and this
//! contract does not refuse a result merely because that state is not the one
//! the transition aimed at. Refusing it would be this contract deciding the
//! author is wrong about its own bundle.

use serde::{Deserialize, Serialize};

use crate::command::platform_service_identity::{BundleState, BundleSymbolicName};
use crate::command::resource_mutation::MutationResultFailure;

/// What one request asks a bundle to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BundleTransition {
    /// Refresh it, rewiring what depends on it.
    Refresh,
    /// Start it.
    Start,
    /// Stop it.
    Stop,
}

/// One request to change a bundle's state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetOpenServiceGatewayInitiativeBundleStateCommand {
    /// The name of the bundle to act on.
    pub symbolic_name: BundleSymbolicName,
    /// What to ask it to do.
    pub transition: BundleTransition,
}

/// Why a bundle's state was not changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SetOpenServiceGatewayInitiativeBundleStateFailure {
    /// No bundle answers to that symbolic name.
    BundleNotFound,
    /// The bundle is there and will not make that transition.
    BundleTransitionRefused,
    /// The author refused the change.
    PlatformControlRejected,
    /// Nobody can tell whether the change took effect.
    PlatformControlOutcomeUnknown,
}

/// One refused bundle transition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetOpenServiceGatewayInitiativeBundleStateRefusal {
    /// Why it was refused.
    pub failure: SetOpenServiceGatewayInitiativeBundleStateFailure,
    /// Bundle this request named.
    pub symbolic_name: BundleSymbolicName,
}

impl SetOpenServiceGatewayInitiativeBundleStateRefusal {
    /// Returns whether this refusal proves the operation changed nothing.
    #[must_use]
    pub fn proves_no_effect(&self) -> bool {
        !matches!(
            self.failure,
            SetOpenServiceGatewayInitiativeBundleStateFailure::PlatformControlOutcomeUnknown
        )
    }

    /// Requires this refusal to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::NotThisRequest`] when it names another
    /// request's bundle.
    pub fn require_answers(
        &self,
        command: &SetOpenServiceGatewayInitiativeBundleStateCommand,
    ) -> Result<(), MutationResultFailure> {
        if self.symbolic_name == command.symbolic_name {
            Ok(())
        } else {
            Err(MutationResultFailure::NotThisRequest)
        }
    }
}

/// What the author observed after the transition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetOpenServiceGatewayInitiativeBundleStateResult {
    /// The state the bundle was in afterwards.
    pub observed_state: BundleState,
    /// Bundle that was acted on.
    pub symbolic_name: BundleSymbolicName,
}

impl SetOpenServiceGatewayInitiativeBundleStateResult {
    /// Requires this result to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`MutationResultFailure::NotThisRequest`] when it names another
    /// request's bundle.
    pub fn require_answers(
        &self,
        command: &SetOpenServiceGatewayInitiativeBundleStateCommand,
    ) -> Result<(), MutationResultFailure> {
        if self.symbolic_name == command.symbolic_name {
            Ok(())
        } else {
            Err(MutationResultFailure::NotThisRequest)
        }
    }
}
