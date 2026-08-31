//! Inspecting one configuration without changing it.
//!
//! One value goes in: the persistent identifier. Nothing else is offered,
//! because everything else a caller might want to ask - which bundle, which
//! factory, which property - is something the operation decides from evidence
//! rather than something a caller may assert.
//!
//! No option asks for a raw value. What
//! comes back is redacted by classification rather than by name, and offering a
//! way to ask around that would be offering a way to read a password.

use slingshot_domain::command::catalog::Command;
use slingshot_domain::command::inspect_open_service_gateway_initiative_configuration::{
    InspectOpenServiceGatewayInitiativeConfigurationCommand,
    OpenServiceGatewayInitiativePersistentIdentifier,
};

use crate::commands::content::{RequestRefusal, require_key, required};
use crate::invocation::Invocation;

/// The wire name of the command this family exposes.
pub const INSPECT_CONFIGURATION: &str = "inspect_open_service_gateway_initiative_configuration";

/// The option naming which configuration to inspect.
pub const IDENTIFIER_OPTION: &str = "--persistent-identifier";

/// Returns the typed request one invocation describes.
///
/// # Errors
///
/// Returns [`RequestRefusal`] naming the first thing that is wrong.
pub fn build(invocation: &Invocation) -> Result<Command, RequestRefusal> {
    if invocation.verb != INSPECT_CONFIGURATION {
        return Err(RequestRefusal::AnotherCommand { named: invocation.verb.clone() });
    }
    require_key(invocation)?;
    let identifier = OpenServiceGatewayInitiativePersistentIdentifier::new(required(
        invocation,
        IDENTIFIER_OPTION,
    )?)
    .map_err(|_| RequestRefusal::ValueUnusable { named: IDENTIFIER_OPTION.to_owned() })?;
    Ok(Command::InspectOpenServiceGatewayInitiativeConfiguration(
        InspectOpenServiceGatewayInitiativeConfigurationCommand {
            persistent_identifier: identifier,
        },
    ))
}
