//! Changing a configuration, and answering with nothing that went into it.
//!
//! This is the platform action an operator takes most often and the one most
//! likely to carry a secret: a password, a token, an endpoint with credentials
//! in it. Values go in because that is the whole point of the command. What
//! comes back is the identifier and a count, and nothing else.
//!
//! That is structural rather than a promise. The result type has no member that
//! could hold an assigned value, so there is no code path that could echo one
//! and no future edit that could add one without changing the type.
//!
//! A key named in both the assignment document and the removal list is refused.
//! There is no order between them a caller could rely on, and choosing one would
//! make the same request mean two things.

use serde::de::Error as _;
use serde::{Deserialize, Serialize};

use crate::command::command_identity::CommandContract;
use crate::command::inspect_open_service_gateway_initiative_configuration::{
    OpenServiceGatewayInitiativeConfigurationPropertyKey,
    OpenServiceGatewayInitiativeConfigurationValue,
    OpenServiceGatewayInitiativePersistentIdentifier, maximum_inspected_configuration_properties,
};
use crate::command::operational_listing::{ListingResultFailure, require_ascending_distinct};

/// Why a configuration update is not one this contract can carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ConfigurationUpdateFailure {
    /// A document names more keys than one configuration may hold.
    #[error("a configuration document is within the property bound its contract declares")]
    TooManyKeys,
    /// A removal list is empty, repeats a key, or is out of order.
    #[error("a removal list is nonempty, distinct, and ascending")]
    RemovalsNotAscendingDistinct,
    /// One key is both assigned and removed by the same request.
    #[error("a key is assigned or removed, and not both by one request")]
    BothAssignedAndRemoved,
    /// The request would change nothing.
    #[error("a configuration update changes something")]
    ChangesNothing,
    /// Two keys in one document are the same key.
    #[error("two keys in one document have different spellings and one identity")]
    DuplicateKeyIdentity,
    /// A result does not answer the command it claims to answer.
    #[error("a configuration result names the configuration its command asked about")]
    NotThisRequest,
}

/// The keys one request assigns, by key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct ConfigurationAssignments {
    /// The values, by key.
    values: std::collections::BTreeMap<
        OpenServiceGatewayInitiativeConfigurationPropertyKey,
        OpenServiceGatewayInitiativeConfigurationValue,
    >,
}

impl ConfigurationAssignments {
    /// Returns the assignments `values` describes.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigurationUpdateFailure::TooManyKeys`] above the bound one
    /// configuration may hold, and
    /// [`ConfigurationUpdateFailure::DuplicateKeyIdentity`] when two keys differ
    /// only in case, which Configuration Admin reads as one key.
    pub fn new(
        values: std::collections::BTreeMap<
            OpenServiceGatewayInitiativeConfigurationPropertyKey,
            OpenServiceGatewayInitiativeConfigurationValue,
        >,
    ) -> Result<Self, ConfigurationUpdateFailure> {
        if u64::try_from(values.len()).unwrap_or(u64::MAX)
            > maximum_inspected_configuration_properties()
        {
            return Err(ConfigurationUpdateFailure::TooManyKeys);
        }
        require_distinct_identities(values.keys())?;
        Ok(Self { values })
    }

    /// Returns the values, by key.
    #[must_use]
    pub fn values(
        &self,
    ) -> &std::collections::BTreeMap<
        OpenServiceGatewayInitiativeConfigurationPropertyKey,
        OpenServiceGatewayInitiativeConfigurationValue,
    > {
        &self.values
    }

    /// Reports whether this document assigns nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

impl<'de> Deserialize<'de> for ConfigurationAssignments {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        let values = std::collections::BTreeMap::deserialize(deserializer)?;
        Self::new(values).map_err(Source::Error::custom)
    }
}

/// The keys one request removes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct RemovedConfigurationKeys {
    /// The keys, ascending and distinct.
    keys: Vec<OpenServiceGatewayInitiativeConfigurationPropertyKey>,
}

impl RemovedConfigurationKeys {
    /// Returns the removal list `keys` describes.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigurationUpdateFailure::TooManyKeys`] above the bound one
    /// configuration may hold,
    /// [`ConfigurationUpdateFailure::RemovalsNotAscendingDistinct`] when the
    /// list is empty, repeats a key, or is out of order, and
    /// [`ConfigurationUpdateFailure::DuplicateKeyIdentity`] when two keys differ
    /// only in case, which Configuration Admin reads as one key.
    pub fn new(
        keys: Vec<OpenServiceGatewayInitiativeConfigurationPropertyKey>,
    ) -> Result<Self, ConfigurationUpdateFailure> {
        let bound = CommandContract::embedded().limit("maximum_inspected_configuration_properties");
        match require_ascending_distinct(&keys, bound) {
            Ok(()) => {
                require_distinct_identities(keys.iter())?;
                Ok(Self { keys })
            }
            Err(ListingResultFailure::TooManyRequested) => {
                Err(ConfigurationUpdateFailure::TooManyKeys)
            }
            Err(_) => Err(ConfigurationUpdateFailure::RemovalsNotAscendingDistinct),
        }
    }

    /// Returns the keys, ascending.
    #[must_use]
    pub fn keys(&self) -> &[OpenServiceGatewayInitiativeConfigurationPropertyKey] {
        &self.keys
    }

    /// Reports whether this list removes `key`.
    ///
    /// By folded identity rather than by spelling, because Configuration Admin
    /// treats `Host` and `host` as one property. Comparing the spellings would
    /// let one request assign a value and remove it, which is the pair this
    /// contract exists to refuse.
    #[must_use]
    pub fn removes(&self, key: &OpenServiceGatewayInitiativeConfigurationPropertyKey) -> bool {
        let identity = key.folded_identity();
        self.keys.iter().any(|removed| removed.folded_identity() == identity)
    }
}

impl<'de> Deserialize<'de> for RemovedConfigurationKeys {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        Self::new(Vec::deserialize(deserializer)?).map_err(Source::Error::custom)
    }
}

/// One request to change a configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateOpenServiceGatewayInitiativeConfigurationCommand {
    /// Values to assign, by key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assignments: Option<ConfigurationAssignments>,
    /// Configuration to change.
    pub persistent_identifier: OpenServiceGatewayInitiativePersistentIdentifier,
    /// Keys to remove.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub removed_property_keys: Option<RemovedConfigurationKeys>,
}

impl UpdateOpenServiceGatewayInitiativeConfigurationCommand {
    /// Requires this request to change exactly one thing per key.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigurationUpdateFailure::BothAssignedAndRemoved`] when one
    /// key is named in both documents, and
    /// [`ConfigurationUpdateFailure::ChangesNothing`] when the request would
    /// change nothing at all.
    pub fn require_usable(&self) -> Result<(), ConfigurationUpdateFailure> {
        if let (Some(assigned), Some(removed)) =
            (self.assignments.as_ref(), self.removed_property_keys.as_ref())
            && assigned.values().keys().any(|key| removed.removes(key))
        {
            return Err(ConfigurationUpdateFailure::BothAssignedAndRemoved);
        }
        let assigns = self.assignments.as_ref().is_some_and(|assigned| !assigned.is_empty());
        if assigns || self.removed_property_keys.is_some() {
            Ok(())
        } else {
            Err(ConfigurationUpdateFailure::ChangesNothing)
        }
    }
}

/// Why a configuration was not changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateOpenServiceGatewayInitiativeConfigurationFailure {
    /// The configuration administration could not be reached.
    ConfigurationLookupFailed,
    /// Nothing is registered under that exact identifier.
    ConfigurationLookupMismatch,
    /// More than one configuration answers to that identifier.
    ConfigurationLookupAmbiguous,
    /// A value is of a class this contract does not carry.
    ConfigurationValueUnsupported,
    /// A value is of a carried class and is not a legal one.
    ConfigurationValueMalformed,
    /// The author refused the change.
    PlatformControlRejected,
    /// Nobody can tell whether the change took effect.
    PlatformControlOutcomeUnknown,
}

/// One refused configuration update.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateOpenServiceGatewayInitiativeConfigurationRefusal {
    /// Why it was refused.
    pub failure: UpdateOpenServiceGatewayInitiativeConfigurationFailure,
    /// Configuration this request named.
    pub persistent_identifier: OpenServiceGatewayInitiativePersistentIdentifier,
}

impl UpdateOpenServiceGatewayInitiativeConfigurationRefusal {
    /// Returns whether this refusal proves the operation changed nothing.
    #[must_use]
    pub fn proves_no_effect(&self) -> bool {
        !matches!(
            self.failure,
            UpdateOpenServiceGatewayInitiativeConfigurationFailure::PlatformControlOutcomeUnknown
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
        command: &UpdateOpenServiceGatewayInitiativeConfigurationCommand,
    ) -> Result<(), ConfigurationUpdateFailure> {
        if self.persistent_identifier == command.persistent_identifier {
            Ok(())
        } else {
            Err(ConfigurationUpdateFailure::NotThisRequest)
        }
    }
}

/// What a completed configuration update changed.
///
/// A count and an identifier. There is no member here that could hold a value,
/// which is the point: what went in never comes back out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateOpenServiceGatewayInitiativeConfigurationResult {
    /// How many keys the update changed.
    pub changed_property_key_count: u64,
    /// Configuration that was changed.
    pub persistent_identifier: OpenServiceGatewayInitiativePersistentIdentifier,
}

impl UpdateOpenServiceGatewayInitiativeConfigurationResult {
    /// Requires this result to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigurationUpdateFailure::NotThisRequest`] when it names
    /// another request's configuration, and
    /// [`ConfigurationUpdateFailure::TooManyKeys`] when it reports changing more
    /// keys than a configuration may hold.
    pub fn require_answers(
        &self,
        command: &UpdateOpenServiceGatewayInitiativeConfigurationCommand,
    ) -> Result<(), ConfigurationUpdateFailure> {
        if self.changed_property_key_count > maximum_inspected_configuration_properties() {
            return Err(ConfigurationUpdateFailure::TooManyKeys);
        }
        if self.persistent_identifier == command.persistent_identifier {
            Ok(())
        } else {
            Err(ConfigurationUpdateFailure::NotThisRequest)
        }
    }
}

/// Requires no two keys in one document to be the same key.
///
/// Configuration Admin treats a property name case-insensitively, so `Host` and
/// `host` are one key written twice. A document carrying both would assign or
/// remove one property under two names, and which of the two won would be the
/// author's accident rather than the caller's request.
fn require_distinct_identities<'key>(
    keys: impl IntoIterator<Item = &'key OpenServiceGatewayInitiativeConfigurationPropertyKey>,
) -> Result<(), ConfigurationUpdateFailure> {
    let mut seen = std::collections::BTreeSet::new();
    for key in keys {
        if !seen.insert(key.folded_identity()) {
            return Err(ConfigurationUpdateFailure::DuplicateKeyIdentity);
        }
    }
    Ok(())
}
