//! Finding configurations when nobody knows the exact identifier.
//!
//! Inspecting a configuration needs its exact persistent identifier, which is
//! the one thing an operator looking for a misconfiguration does not have. This
//! command finds them by prefix.
//!
//! # Why no row carries a value
//!
//! Reading a configuration value is allowed only behind the metatype evidence
//! the inspection command gathers and the redaction it applies, and that is a
//! judgement made per identifier against what the deployment declares. A listing
//! has made none of those judgements, so it reports identifiers, whether the
//! configuration is bound to a bundle location, and how many keys it has - facts
//! about the configuration's shape, never about its contents. The match type has
//! no member that could carry a value, so this is structural rather than a
//! promise somebody has to keep remembering.

use serde::de::Error as _;
use serde::{Deserialize, Serialize};

use crate::command::inspect_open_service_gateway_initiative_configuration::{
    ConfigurationFailure, OpenServiceGatewayInitiativePersistentIdentifier,
    maximum_inspected_configuration_properties,
};
use crate::command::operational_listing::{ListingResultFailure, require_strictly_ascending_text};
use crate::command::result_window::{ContinuationToken, ResultWindow};

/// One request to find configurations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FindOpenServiceGatewayInitiativeConfigurationsCommand {
    /// Prefix every reported identifier begins with, when the caller said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persistent_identifier_prefix: Option<OpenServiceGatewayInitiativePersistentIdentifier>,
    /// Page the caller is asking for, when the caller said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_window: Option<ResultWindow>,
}

impl FindOpenServiceGatewayInitiativeConfigurationsCommand {
    /// Returns the page this request asks for, stated or resolved.
    #[must_use]
    pub fn resolved_window(&self) -> ResultWindow {
        self.result_window.clone().unwrap_or_default()
    }

    /// Returns whether `identifier` is one this request asked about.
    #[must_use]
    pub fn admits(&self, identifier: &OpenServiceGatewayInitiativePersistentIdentifier) -> bool {
        self.persistent_identifier_prefix
            .as_ref()
            .is_none_or(|prefix| identifier.as_text().starts_with(prefix.as_text()))
    }
}

/// One configuration, described without reading anything it holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConfigurationMatch {
    /// Whether the configuration is bound to one bundle's location.
    pub bound_to_a_bundle_location: bool,
    /// Factory this configuration was made from, when it was made from one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub factory_persistent_identifier: Option<OpenServiceGatewayInitiativePersistentIdentifier>,
    /// Configuration this row is about.
    pub persistent_identifier: OpenServiceGatewayInitiativePersistentIdentifier,
    /// How many keys it holds, which is a shape and not a content.
    pub property_key_count: u64,
}

impl ConfigurationMatch {
    /// Returns the row these facts describe.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigurationFailure::TooManyProperties`] when the key count
    /// exceeds what one configuration may hold.
    pub fn new(
        bound_to_a_bundle_location: bool,
        factory_persistent_identifier: Option<OpenServiceGatewayInitiativePersistentIdentifier>,
        persistent_identifier: OpenServiceGatewayInitiativePersistentIdentifier,
        property_key_count: u64,
    ) -> Result<Self, ConfigurationFailure> {
        if property_key_count > maximum_inspected_configuration_properties() {
            return Err(ConfigurationFailure::TooManyProperties);
        }
        Ok(Self {
            bound_to_a_bundle_location,
            factory_persistent_identifier,
            persistent_identifier,
            property_key_count,
        })
    }
}

/// One page of configurations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FindOpenServiceGatewayInitiativeConfigurationsResult {
    /// Matches, strictly ascending by persistent identifier bytes.
    pub matches: Vec<ConfigurationMatch>,
    /// Where the next page resumes, when there is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_continuation_token: Option<ContinuationToken>,
}

impl FindOpenServiceGatewayInitiativeConfigurationsResult {
    /// Returns the page these matches describe.
    ///
    /// # Errors
    ///
    /// Returns [`ListingResultFailure::NotStrictlyAscending`] when an identifier
    /// repeats or sorts before its predecessor.
    pub fn new(
        matches: Vec<ConfigurationMatch>,
        next_continuation_token: Option<ContinuationToken>,
    ) -> Result<Self, ListingResultFailure> {
        require_strictly_ascending_text(
            matches.iter().map(|found| found.persistent_identifier.as_text()),
        )?;
        Ok(Self { matches, next_continuation_token })
    }

    /// Requires this page to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`ListingResultFailure::NotThisRequest`] when a match does not
    /// carry the prefix the command asked about.
    pub fn require_answers(
        &self,
        command: &FindOpenServiceGatewayInitiativeConfigurationsCommand,
    ) -> Result<(), ListingResultFailure> {
        let admitted =
            self.matches.iter().all(|found| command.admits(&found.persistent_identifier));
        if admitted { Ok(()) } else { Err(ListingResultFailure::NotThisRequest) }
    }
}

/// One match exactly as it is written on the wire.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MatchDocument {
    /// Whether the configuration is bound to one bundle's location.
    bound_to_a_bundle_location: bool,
    /// Factory this configuration was made from.
    #[serde(default)]
    factory_persistent_identifier: Option<OpenServiceGatewayInitiativePersistentIdentifier>,
    /// Configuration this row is about.
    persistent_identifier: OpenServiceGatewayInitiativePersistentIdentifier,
    /// How many keys it holds.
    property_key_count: u64,
}

impl<'de> Deserialize<'de> for ConfigurationMatch {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        let document = MatchDocument::deserialize(deserializer)?;
        Self::new(
            document.bound_to_a_bundle_location,
            document.factory_persistent_identifier,
            document.persistent_identifier,
            document.property_key_count,
        )
        .map_err(Source::Error::custom)
    }
}

/// One page exactly as it is written on the wire.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ResultDocument {
    /// Matches this page carries.
    matches: Vec<ConfigurationMatch>,
    /// Where the next page resumes.
    #[serde(default)]
    next_continuation_token: Option<ContinuationToken>,
}

impl<'de> Deserialize<'de> for FindOpenServiceGatewayInitiativeConfigurationsResult {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        let document = ResultDocument::deserialize(deserializer)?;
        Self::new(document.matches, document.next_continuation_token).map_err(Source::Error::custom)
    }
}
