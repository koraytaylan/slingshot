//! Finding and changing configurations, and looking at bundles and components.
//!
//! `--prefix` filters every listing here, and what it is a prefix of is whatever
//! that listing is ordered by: a persistent identifier, a symbolic name, a
//! component name. One option rather than three named after the same idea.
//!
//! `--assignments` carries one JSON document of typed values, for the reason the
//! element document is one: the domain declares what a configuration value is,
//! and a command-line grammar for it here would be a second declaration that
//! could disagree.

use slingshot_domain::command::catalog::Command;
use slingshot_domain::command::delete_open_service_gateway_initiative_configuration::DeleteOpenServiceGatewayInitiativeConfigurationCommand;
use slingshot_domain::command::find_open_service_gateway_initiative_configurations::FindOpenServiceGatewayInitiativeConfigurationsCommand;
use slingshot_domain::command::inspect_open_service_gateway_initiative_configuration::OpenServiceGatewayInitiativePersistentIdentifier;
use slingshot_domain::command::list_open_service_gateway_initiative_bundles::ListOpenServiceGatewayInitiativeBundlesCommand;
use slingshot_domain::command::list_open_service_gateway_initiative_components::ListOpenServiceGatewayInitiativeComponentsCommand;
use slingshot_domain::command::platform_service_identity::{
    BundleState, BundleSymbolicName, ComponentState, DeclarativeServiceComponentName,
    RequestedBundleStates, RequestedComponentStates,
};
use slingshot_domain::command::set_open_service_gateway_initiative_bundle_state::{
    BundleTransition, SetOpenServiceGatewayInitiativeBundleStateCommand,
};
use slingshot_domain::command::update_open_service_gateway_initiative_configuration::{
    ConfigurationAssignments, RemovedConfigurationKeys,
    UpdateOpenServiceGatewayInitiativeConfigurationCommand,
};

use crate::commands::content::{RequestRefusal, require_key, required};
use crate::commands::operational_values::{list, optional_document, optional_text, unusable};
use crate::commands::path_query::window;
use crate::invocation::{
    ASSIGNMENTS_OPTION, Invocation, PERSISTENT_IDENTIFIER_OPTION, PREFIX_OPTION,
    REMOVED_KEYS_OPTION, STATES_OPTION, SYMBOLIC_NAME_OPTION, TRANSITION_OPTION,
};

/// The wire name of the configuration search.
pub const FIND_CONFIGURATIONS: &str = "find_open_service_gateway_initiative_configurations";

/// The wire name of the configuration update.
pub const UPDATE_CONFIGURATION: &str = "update_open_service_gateway_initiative_configuration";

/// The wire name of the configuration removal.
pub const DELETE_CONFIGURATION: &str = "delete_open_service_gateway_initiative_configuration";

/// The wire name of the bundle listing.
pub const LIST_BUNDLES: &str = "list_open_service_gateway_initiative_bundles";

/// The wire name of the bundle transition.
pub const SET_BUNDLE_STATE: &str = "set_open_service_gateway_initiative_bundle_state";

/// The wire name of the component listing.
pub const LIST_COMPONENTS: &str = "list_open_service_gateway_initiative_components";

/// Every command this family builds.
const NAMES: &[&str] = &[
    FIND_CONFIGURATIONS,
    UPDATE_CONFIGURATION,
    DELETE_CONFIGURATION,
    LIST_BUNDLES,
    SET_BUNDLE_STATE,
    LIST_COMPONENTS,
];

/// Returns the typed request one invocation describes.
///
/// # Errors
///
/// Returns [`RequestRefusal`] naming the first thing that is wrong, or that this
/// family builds no such command.
pub fn build(invocation: &Invocation) -> Result<Command, RequestRefusal> {
    if !NAMES.contains(&invocation.verb.as_str()) {
        return Err(RequestRefusal::AnotherCommand { named: invocation.verb.clone() });
    }
    require_key(invocation)?;
    build_configuration(invocation).unwrap_or_else(|| build_platform(invocation))
}

/// Returns the configuration command one invocation describes, when it is one.
fn build_configuration(invocation: &Invocation) -> Option<Result<Command, RequestRefusal>> {
    let built = match invocation.verb.as_str() {
        FIND_CONFIGURATIONS => find(invocation),
        UPDATE_CONFIGURATION => update(invocation),
        DELETE_CONFIGURATION => delete(invocation),
        _ => return None,
    };
    Some(built)
}

/// Returns the bundle or component command one invocation describes.
fn build_platform(invocation: &Invocation) -> Result<Command, RequestRefusal> {
    match invocation.verb.as_str() {
        LIST_BUNDLES => list_bundles(invocation),
        SET_BUNDLE_STATE => set_bundle_state(invocation),
        _ => list_components(invocation),
    }
}

/// Returns the configuration search one invocation describes.
fn find(invocation: &Invocation) -> Result<Command, RequestRefusal> {
    let prefix = optional_text(invocation, PREFIX_OPTION)
        .map(|stated| {
            OpenServiceGatewayInitiativePersistentIdentifier::new(stated)
                .map_err(|_| unusable(PREFIX_OPTION))
        })
        .transpose()?;
    Ok(Command::FindOpenServiceGatewayInitiativeConfigurations(
        FindOpenServiceGatewayInitiativeConfigurationsCommand {
            persistent_identifier_prefix: prefix,
            result_window: window(invocation)?,
        },
    ))
}

/// Returns the configuration update one invocation describes.
fn update(invocation: &Invocation) -> Result<Command, RequestRefusal> {
    let assignments: Option<ConfigurationAssignments> =
        optional_document(invocation, ASSIGNMENTS_OPTION)?;
    let removed_property_keys: Option<RemovedConfigurationKeys> =
        optional_document(invocation, REMOVED_KEYS_OPTION)?;
    Ok(Command::UpdateOpenServiceGatewayInitiativeConfiguration(
        UpdateOpenServiceGatewayInitiativeConfigurationCommand {
            assignments,
            persistent_identifier: identifier(invocation)?,
            removed_property_keys,
        },
    ))
}

/// Returns the configuration removal one invocation describes.
fn delete(invocation: &Invocation) -> Result<Command, RequestRefusal> {
    Ok(Command::DeleteOpenServiceGatewayInitiativeConfiguration(
        DeleteOpenServiceGatewayInitiativeConfigurationCommand {
            persistent_identifier: identifier(invocation)?,
        },
    ))
}

/// Returns the bundle listing one invocation describes.
fn list_bundles(invocation: &Invocation) -> Result<Command, RequestRefusal> {
    let states = invocation
        .arguments
        .contains_key(STATES_OPTION)
        .then(|| {
            let states: Vec<BundleState> = list(invocation, STATES_OPTION)?;
            RequestedBundleStates::new(states).map_err(|_| unusable(STATES_OPTION))
        })
        .transpose()?;
    let symbolic_name_prefix = optional_text(invocation, PREFIX_OPTION)
        .map(|stated| BundleSymbolicName::parse(&stated).map_err(|_| unusable(PREFIX_OPTION)))
        .transpose()?;
    Ok(Command::ListOpenServiceGatewayInitiativeBundles(
        ListOpenServiceGatewayInitiativeBundlesCommand {
            result_window: window(invocation)?,
            states,
            symbolic_name_prefix,
        },
    ))
}

/// Returns the bundle transition one invocation describes.
fn set_bundle_state(invocation: &Invocation) -> Result<Command, RequestRefusal> {
    let transition: BundleTransition =
        spelled(required(invocation, TRANSITION_OPTION)?, TRANSITION_OPTION)?;
    Ok(Command::SetOpenServiceGatewayInitiativeBundleState(
        SetOpenServiceGatewayInitiativeBundleStateCommand {
            symbolic_name: BundleSymbolicName::parse(required(invocation, SYMBOLIC_NAME_OPTION)?)
                .map_err(|_| unusable(SYMBOLIC_NAME_OPTION))?,
            transition,
        },
    ))
}

/// Returns the component listing one invocation describes.
fn list_components(invocation: &Invocation) -> Result<Command, RequestRefusal> {
    let states = invocation
        .arguments
        .contains_key(STATES_OPTION)
        .then(|| {
            let states: Vec<ComponentState> = list(invocation, STATES_OPTION)?;
            RequestedComponentStates::new(states).map_err(|_| unusable(STATES_OPTION))
        })
        .transpose()?;
    let name_prefix = optional_text(invocation, PREFIX_OPTION)
        .map(|stated| {
            DeclarativeServiceComponentName::parse(&stated).map_err(|_| unusable(PREFIX_OPTION))
        })
        .transpose()?;
    Ok(Command::ListOpenServiceGatewayInitiativeComponents(
        ListOpenServiceGatewayInitiativeComponentsCommand {
            name_prefix,
            result_window: window(invocation)?,
            states,
        },
    ))
}

/// Returns the configuration one invocation names.
fn identifier(
    invocation: &Invocation,
) -> Result<OpenServiceGatewayInitiativePersistentIdentifier, RequestRefusal> {
    OpenServiceGatewayInitiativePersistentIdentifier::new(required(
        invocation,
        PERSISTENT_IDENTIFIER_OPTION,
    )?)
    .map_err(|_| unusable(PERSISTENT_IDENTIFIER_OPTION))
}

/// Returns the closed value `stated` spells.
fn spelled<Target: serde::de::DeserializeOwned>(
    stated: &str,
    option: &str,
) -> Result<Target, RequestRefusal> {
    serde_json::from_str(&format!("\"{stated}\"")).map_err(|_| unusable(option))
}
