//! Replicating content to another environment.
//!
//! Two values and no more: what to replicate, and whether to include what is
//! below it. Everything else about a replication - which agent carries it, how
//! the far side admits it, what an unknown admission means - belongs to the
//! operation rather than to the words a caller types, and offering an option
//! for any of it here would be offering a choice this surface cannot honour.
//!
//! Recursion is a flag with a default of off, because the difference between
//! one node and a subtree is the difference between a small change and a large
//! one, and the large one should be asked for.

use slingshot_domain::command::catalog::Command;
use slingshot_domain::command::replicate_content::ReplicateContentCommand;
use slingshot_domain::command::repository_path::RepositoryPath;

use crate::commands::content::{RequestRefusal, require_key, required};
use crate::invocation::{Invocation, PATH_OPTION};

/// The wire name of the command this family exposes.
pub const REPLICATE_CONTENT: &str = "replicate_content";

/// The option that includes everything below the named path.
pub const RECURSIVE_OPTION: &str = "--recursive";

/// Returns the typed request one invocation describes.
///
/// # Errors
///
/// Returns [`RequestRefusal`] naming the first thing that is wrong.
pub fn build(invocation: &Invocation) -> Result<Command, RequestRefusal> {
    if invocation.verb != REPLICATE_CONTENT {
        return Err(RequestRefusal::AnotherCommand { named: invocation.verb.clone() });
    }
    require_key(invocation)?;
    let path = RepositoryPath::parse(required(invocation, PATH_OPTION)?)
        .map_err(|_| RequestRefusal::ValueUnusable { named: PATH_OPTION.to_owned() })?;
    Ok(Command::ReplicateContent(ReplicateContentCommand {
        path,
        recursive: invocation.arguments.contains_key(RECURSIVE_OPTION),
    }))
}
