//! What is wired, and what is waiting for something that never arrived.
//!
//! A bundle can be active while the component that matters is unsatisfied, and
//! that is usually the state an operator is hunting. Listing bundles cannot show
//! it, because a bundle's state says nothing about whether the components inside
//! it found their references.
//!
//! Each row names the bundle that declares the component, so a caller who finds
//! an unsatisfied component knows immediately which bundle to look at next.

use serde::de::Error as _;
use serde::{Deserialize, Serialize};

use crate::command::inspect_open_service_gateway_initiative_configuration::OpenServiceGatewayInitiativePersistentIdentifier;
use crate::command::operational_listing::{ListingResultFailure, require_strictly_ascending_text};
use crate::command::platform_service_identity::{
    BundleSymbolicName, ComponentState, DeclarativeServiceComponentName, RequestedComponentStates,
};
use crate::command::result_window::{ContinuationToken, ResultWindow};

/// One request to list declarative service components.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListOpenServiceGatewayInitiativeComponentsCommand {
    /// Prefix every reported component name begins with, when the caller said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name_prefix: Option<DeclarativeServiceComponentName>,
    /// Page the caller is asking for, when the caller said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_window: Option<ResultWindow>,
    /// States a reported component may be in, when the caller said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub states: Option<RequestedComponentStates>,
}

impl ListOpenServiceGatewayInitiativeComponentsCommand {
    /// Returns the page this request asks for, stated or resolved.
    #[must_use]
    pub fn resolved_window(&self) -> ResultWindow {
        self.result_window.clone().unwrap_or_default()
    }

    /// Returns whether a component so named and so stated is one this request
    /// asked about.
    #[must_use]
    pub fn admits(&self, name: &DeclarativeServiceComponentName, state: ComponentState) -> bool {
        let named = self
            .name_prefix
            .as_ref()
            .is_none_or(|prefix| name.as_text().starts_with(prefix.as_text()));
        let stated = self.states.as_ref().is_none_or(|states| states.contains(state));
        named && stated
    }
}

/// One declarative service component the author reports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentMatch {
    /// The bundle that declares it.
    pub bundle_symbolic_name: BundleSymbolicName,
    /// The name it is addressed by.
    pub name: DeclarativeServiceComponentName,
    /// The configuration it reads, when it names one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_persistent_identifier: Option<OpenServiceGatewayInitiativePersistentIdentifier>,
    /// What state it is in.
    pub state: ComponentState,
}

/// One page of components.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ListOpenServiceGatewayInitiativeComponentsResult {
    /// Matches, strictly ascending by component name bytes.
    pub matches: Vec<ComponentMatch>,
    /// Where the next page resumes, when there is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_continuation_token: Option<ContinuationToken>,
}

impl ListOpenServiceGatewayInitiativeComponentsResult {
    /// Returns the page these matches describe.
    ///
    /// # Errors
    ///
    /// Returns [`ListingResultFailure::NotStrictlyAscending`] when a name
    /// repeats or sorts before its predecessor.
    pub fn new(
        matches: Vec<ComponentMatch>,
        next_continuation_token: Option<ContinuationToken>,
    ) -> Result<Self, ListingResultFailure> {
        require_strictly_ascending_text(matches.iter().map(|found| found.name.as_text()))?;
        Ok(Self { matches, next_continuation_token })
    }

    /// Requires this page to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`ListingResultFailure::NotThisRequest`] when a match is outside
    /// the prefix or the states the command asked about.
    pub fn require_answers(
        &self,
        command: &ListOpenServiceGatewayInitiativeComponentsCommand,
    ) -> Result<(), ListingResultFailure> {
        let admitted = self.matches.iter().all(|found| command.admits(&found.name, found.state));
        if admitted { Ok(()) } else { Err(ListingResultFailure::NotThisRequest) }
    }
}

/// One page exactly as it is written on the wire.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ResultDocument {
    /// Matches this page carries.
    matches: Vec<ComponentMatch>,
    /// Where the next page resumes.
    #[serde(default)]
    next_continuation_token: Option<ContinuationToken>,
}

impl<'de> Deserialize<'de> for ListOpenServiceGatewayInitiativeComponentsResult {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        let document = ResultDocument::deserialize(deserializer)?;
        Self::new(document.matches, document.next_continuation_token).map_err(Source::Error::custom)
    }
}
