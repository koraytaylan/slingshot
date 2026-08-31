//! Loading repository content as a document.
//!
//! One command, and the interesting thing about it is that a read needs a
//! caller's operation key. Loading a document is read-only and it is not
//! intrinsically idempotent: it produces an artifact whose retention is charged
//! against a target, so running it twice is two pieces of work even though it
//! changes nothing in the repository. The registry says so, and this surface
//! reads that classification rather than inferring it from the read label.
//!
//! The key is required before anything external is touched. A caller who forgot
//! it gets a message rather than a submitted operation they now have to find
//! and abandon.

use slingshot_domain::command::catalog::Command;
use slingshot_domain::command::load_content_as_javascript_object_notation::{
    LoadContentAsJavaScriptObjectNotationCommand, LoadDepth,
};
use slingshot_domain::command::repository_path::RepositoryPath;

use crate::invocation::{Invocation, requires_operation_key};

/// The wire name of the command this family exposes.
pub const LOAD_CONTENT: &str = "load_content_as_json";

/// The option naming the resource to load.
pub const PATH_OPTION: &str = "--path";

/// The option naming how far below it to reach.
pub const DEPTH_OPTION: &str = "--depth";

/// Why one invocation is not a request.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RequestRefusal {
    /// The invocation names another command.
    #[error("{named} is not a command this family builds")]
    AnotherCommand {
        /// What was asked for.
        named: String,
    },
    /// A required option was not supplied.
    #[error("{named} is required, and this invocation does not carry it")]
    OptionMissing {
        /// Which option.
        named: String,
    },
    /// A supplied value is not one the domain accepts.
    #[error("{named} is not a value this command accepts")]
    ValueUnusable {
        /// Which option carried it.
        named: String,
    },
    /// The command changes what is charged and no key was supplied.
    #[error("{named} needs the caller key that makes a repeat the same request")]
    OperationKeyRequired {
        /// Which command.
        named: String,
    },
}

/// Returns the typed request one invocation describes.
///
/// The key is checked first, before any value is parsed and long before
/// anything is opened or connected to, so a caller who forgot it is told rather
/// than discovering it after the work started.
///
/// # Errors
///
/// Returns [`RequestRefusal`] naming the first thing that is wrong.
pub fn build(invocation: &Invocation) -> Result<Command, RequestRefusal> {
    if invocation.verb != LOAD_CONTENT {
        return Err(RequestRefusal::AnotherCommand { named: invocation.verb.clone() });
    }
    require_key(invocation)?;
    let path = required(invocation, PATH_OPTION)?;
    let path = RepositoryPath::parse(path)
        .map_err(|_| RequestRefusal::ValueUnusable { named: PATH_OPTION.to_owned() })?;
    let depth = match invocation.arguments.get(DEPTH_OPTION) {
        Some(stated) => Some(parse_depth(stated)?),
        None => None,
    };
    Ok(Command::LoadContentAsJson(LoadContentAsJavaScriptObjectNotationCommand { depth, path }))
}

/// Requires this invocation to carry the key its command needs.
///
/// # Errors
///
/// Returns [`RequestRefusal::OperationKeyRequired`].
pub fn require_key(invocation: &Invocation) -> Result<(), RequestRefusal> {
    if requires_operation_key(&invocation.verb) && invocation.operation_key.is_none() {
        return Err(RequestRefusal::OperationKeyRequired { named: invocation.verb.clone() });
    }
    Ok(())
}

/// Returns the value of one option a command cannot do without.
///
/// # Errors
///
/// Returns [`RequestRefusal::OptionMissing`].
pub fn required<'invocation>(
    invocation: &'invocation Invocation,
    named: &str,
) -> Result<&'invocation str, RequestRefusal> {
    invocation
        .arguments
        .get(named)
        .map(String::as_str)
        .ok_or_else(|| RequestRefusal::OptionMissing { named: named.to_owned() })
}

/// Returns the depth `stated` names.
fn parse_depth(stated: &str) -> Result<LoadDepth, RequestRefusal> {
    let unusable = || RequestRefusal::ValueUnusable { named: DEPTH_OPTION.to_owned() };
    let value: u64 = stated.parse().map_err(|_| unusable())?;
    LoadDepth::new(value).map_err(|_| unusable())
}
