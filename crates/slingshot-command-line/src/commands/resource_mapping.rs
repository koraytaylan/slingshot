//! Listing the mapping, and asking it a question in either direction.
//!
//! Three commands and one shared decision: `--include-trace` says whether the
//! answer carries the entries that produced it. It is a flag rather than a
//! default because the trace's size depends on the deployment, and a caller who
//! did not ask for one should not have their answer grow because somebody added
//! mapping entries.

use slingshot_domain::command::catalog::Command;
use slingshot_domain::command::list_resource_mappings::ListResourceMappingsCommand;
use slingshot_domain::command::resource_mapping_entry::RequestAddress;
use slingshot_domain::command::resource_resolution::{
    MapResourcePathCommand, ResolveResourcePathCommand,
};

use crate::commands::content::{RequestRefusal, require_key, required};
use crate::commands::operational_values::{flag, optional_text, path, unusable};
use crate::commands::path_query::window;
use crate::invocation::{
    INCLUDE_TRACE_OPTION, Invocation, PATH_OPTION, REQUEST_ADDRESS_OPTION, REQUEST_AUTHORITY_OPTION,
};

/// The wire name of the mapping listing.
pub const LIST_RESOURCE_MAPPINGS: &str = "list_resource_mappings";

/// The wire name of the resolution.
pub const RESOLVE_RESOURCE_PATH: &str = "resolve_resource_path";

/// The wire name of the mapping.
pub const MAP_RESOURCE_PATH: &str = "map_resource_path";

/// Every command this family builds.
const NAMES: &[&str] = &[LIST_RESOURCE_MAPPINGS, RESOLVE_RESOURCE_PATH, MAP_RESOURCE_PATH];

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
        LIST_RESOURCE_MAPPINGS => Ok(Command::ListResourceMappings(ListResourceMappingsCommand {
            result_window: window(invocation)?,
        })),
        RESOLVE_RESOURCE_PATH => Ok(Command::ResolveResourcePath(ResolveResourcePathCommand {
            include_trace: flag(invocation, INCLUDE_TRACE_OPTION),
            request_address: RequestAddress::parse(required(invocation, REQUEST_ADDRESS_OPTION)?)
                .map_err(|_| unusable(REQUEST_ADDRESS_OPTION))?,
        })),
        _ => Ok(Command::MapResourcePath(MapResourcePathCommand {
            include_trace: flag(invocation, INCLUDE_TRACE_OPTION),
            repository_path: path(invocation, PATH_OPTION)?,
            request_authority: optional_text(invocation, REQUEST_AUTHORITY_OPTION),
        })),
    }
}
