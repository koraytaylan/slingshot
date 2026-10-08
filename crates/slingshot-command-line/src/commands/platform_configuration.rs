//! Finding configurations, and looking at bundles and components.
//!
//! `--prefix` filters every listing here, and what it is a prefix of is whatever
//! that listing is ordered by: a persistent identifier, a symbolic name, a
//! component name. One option rather than three named after the same idea.
//!
//! Nothing here changes the running platform: its configurations and bundles are
//! decided by deployments.

use slingshot_domain::command::catalog::Command;
use slingshot_domain::command::find_open_service_gateway_initiative_configurations::FindOpenServiceGatewayInitiativeConfigurationsCommand;
use slingshot_domain::command::inspect_open_service_gateway_initiative_configuration::OpenServiceGatewayInitiativePersistentIdentifier;
use slingshot_domain::command::list_open_service_gateway_initiative_bundles::ListOpenServiceGatewayInitiativeBundlesCommand;
use slingshot_domain::command::list_open_service_gateway_initiative_components::ListOpenServiceGatewayInitiativeComponentsCommand;
use slingshot_domain::command::platform_service_identity::{
    BundleState, BundleSymbolicName, ComponentState, DeclarativeServiceComponentName,
    RequestedBundleStates, RequestedComponentStates,
};

use crate::commands::content::{RequestRefusal, require_key};
use crate::commands::operational_values::{list, optional_text, unusable};
use crate::commands::path_query::window;
use crate::invocation::{Invocation, PREFIX_OPTION, STATES_OPTION};

/// The wire name of the configuration search.
pub const FIND_CONFIGURATIONS: &str = "find_open_service_gateway_initiative_configurations";

/// The wire name of the bundle listing.
pub const LIST_BUNDLES: &str = "list_open_service_gateway_initiative_bundles";

/// The wire name of the component listing.
pub const LIST_COMPONENTS: &str = "list_open_service_gateway_initiative_components";

/// Every command this family builds.
const NAMES: &[&str] = &[FIND_CONFIGURATIONS, LIST_BUNDLES, LIST_COMPONENTS];

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
    match invocation.verb.as_str() {
        FIND_CONFIGURATIONS => find(invocation),
        LIST_BUNDLES => list_bundles(invocation),
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
