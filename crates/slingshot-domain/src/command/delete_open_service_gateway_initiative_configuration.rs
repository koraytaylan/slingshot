//! Removing a configuration, which restores whatever the code defaults to.
//!
//! Deleting a configuration is not deleting a setting; it is handing the
//! decision back to whatever default the code carries, which may be a different
//! value on a different build. That is a state change an operator should make on
//! purpose, so an absent configuration is a failure rather than a success with
//! nothing to do.
//!
//! The result reports whether the configuration came from a factory, because
//! that changes what the deletion meant: removing a factory instance removes an
//! instance, while removing an ordinary configuration reverts one.

use serde::{Deserialize, Serialize};

use crate::command::inspect_open_service_gateway_initiative_configuration::OpenServiceGatewayInitiativePersistentIdentifier;
use crate::command::update_open_service_gateway_initiative_configuration::ConfigurationUpdateFailure;

/// One request to remove a configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeleteOpenServiceGatewayInitiativeConfigurationCommand {
    /// Configuration to remove.
    pub persistent_identifier: OpenServiceGatewayInitiativePersistentIdentifier,
}

/// Why a configuration was not removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeleteOpenServiceGatewayInitiativeConfigurationFailure {
    /// The configuration administration could not be reached.
    ConfigurationLookupFailed,
    /// Nothing is registered under that exact identifier.
    ConfigurationLookupMismatch,
    /// More than one configuration answers to that identifier.
    ConfigurationLookupAmbiguous,
    /// The author refused the removal.
    PlatformControlRejected,
    /// Nobody can tell whether the removal took effect.
    PlatformControlOutcomeUnknown,
}

/// One refused configuration removal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeleteOpenServiceGatewayInitiativeConfigurationRefusal {
    /// Why it was refused.
    pub failure: DeleteOpenServiceGatewayInitiativeConfigurationFailure,
    /// Configuration this request named.
    pub persistent_identifier: OpenServiceGatewayInitiativePersistentIdentifier,
}

impl DeleteOpenServiceGatewayInitiativeConfigurationRefusal {
    /// Returns whether this refusal proves the operation changed nothing.
    #[must_use]
    pub fn proves_no_effect(&self) -> bool {
        !matches!(
            self.failure,
            DeleteOpenServiceGatewayInitiativeConfigurationFailure::PlatformControlOutcomeUnknown
        )
    }

    /// Requires this refusal to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigurationUpdateFailure::NotThisRequest`] when it names
    /// another request's configuration.
    pub fn require_answers(
        &self,
        command: &DeleteOpenServiceGatewayInitiativeConfigurationCommand,
    ) -> Result<(), ConfigurationUpdateFailure> {
        if self.persistent_identifier == command.persistent_identifier {
            Ok(())
        } else {
            Err(ConfigurationUpdateFailure::NotThisRequest)
        }
    }
}

/// What a completed configuration removal did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeleteOpenServiceGatewayInitiativeConfigurationResult {
    /// Whether the removed configuration was one instance of a factory.
    pub was_a_factory_instance: bool,
    /// Configuration that was removed.
    pub persistent_identifier: OpenServiceGatewayInitiativePersistentIdentifier,
}

impl DeleteOpenServiceGatewayInitiativeConfigurationResult {
    /// Requires this result to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigurationUpdateFailure::NotThisRequest`] when it names
    /// another request's configuration.
    pub fn require_answers(
        &self,
        command: &DeleteOpenServiceGatewayInitiativeConfigurationCommand,
    ) -> Result<(), ConfigurationUpdateFailure> {
        if self.persistent_identifier == command.persistent_identifier {
            Ok(())
        } else {
            Err(ConfigurationUpdateFailure::NotThisRequest)
        }
    }
}
