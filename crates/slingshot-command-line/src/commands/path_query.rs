//! Finding paths under a root by their properties.
//!
//! A root, an optional primary type, the shared predicate grammar, and a
//! window. The predicates come from one parser shared with every discovery
//! command, because a second grammar for the same questions would drift on
//! exactly the values where drifting matters.
//!
//! A window is either an offset and a limit or a continuation token, never
//! both. A token already carries the window it was issued under, so accepting
//! one beside a fresh offset would let a caller widen a page the token was
//! bound to.

use slingshot_domain::command::catalog::Command;
use slingshot_domain::command::query_paths::QueryPathsCommand;
use slingshot_domain::command::repository_path::{PrimaryNodeTypeName, RepositoryPath};
use slingshot_domain::command::result_window::ResultWindow;

use crate::commands::content::{RequestRefusal, require_key, required};
use crate::invocation::{
    CONTINUATION_TOKEN_OPTION, Invocation, LIMIT_OPTION, NODE_TYPE_OPTION, OFFSET_OPTION,
    PATH_OPTION,
};
use crate::predicate_arguments::{PREDICATE_OPTION, parse_all};

/// The wire name of the command this family exposes.
pub const QUERY_PATHS: &str = "query_paths";

/// Returns the typed request one invocation describes.
///
/// # Errors
///
/// Returns [`RequestRefusal`] naming the first thing that is wrong.
pub fn build(invocation: &Invocation) -> Result<Command, RequestRefusal> {
    if invocation.verb != QUERY_PATHS {
        return Err(RequestRefusal::AnotherCommand { named: invocation.verb.clone() });
    }
    require_key(invocation)?;
    let root_path = RepositoryPath::parse(required(invocation, PATH_OPTION)?)
        .map_err(|_| RequestRefusal::ValueUnusable { named: PATH_OPTION.to_owned() })?;
    let primary_node_type =
        match invocation.arguments.get(NODE_TYPE_OPTION) {
            Some(stated) => Some(PrimaryNodeTypeName::parse(stated).map_err(|_| {
                RequestRefusal::ValueUnusable { named: NODE_TYPE_OPTION.to_owned() }
            })?),
            None => None,
        };
    Ok(Command::QueryPaths(QueryPathsCommand {
        primary_node_type,
        property_predicates: predicates(invocation)?,
        result_window: window(invocation)?,
        root_path,
    }))
}

/// Returns the predicates one invocation carries, when it carries any.
///
/// # Errors
///
/// Returns [`RequestRefusal::ValueUnusable`] naming the predicate option.
pub fn predicates(
    invocation: &Invocation,
) -> Result<Option<slingshot_domain::command::search_predicate::PropertyPredicates>, RequestRefusal>
{
    let Some(stated) = invocation.arguments.get(PREDICATE_OPTION) else {
        return Ok(None);
    };
    parse_all(std::slice::from_ref(stated))
        .map(Some)
        .map_err(|_| RequestRefusal::ValueUnusable { named: PREDICATE_OPTION.to_owned() })
}

/// Returns the window one invocation asks for, when it asks for one.
///
/// # Errors
///
/// Returns [`RequestRefusal::ValueUnusable`] when the two forms are combined or
/// a value is not one the domain admits. A token already carries the window it
/// was issued under, so accepting one beside a fresh offset would let a caller
/// widen the page the token was bound to.
pub fn window(invocation: &Invocation) -> Result<Option<ResultWindow>, RequestRefusal> {
    let token = invocation.arguments.get(CONTINUATION_TOKEN_OPTION);
    let offset = invocation.arguments.get(OFFSET_OPTION);
    let limit = invocation.arguments.get(LIMIT_OPTION);
    let unusable = |named: &str| RequestRefusal::ValueUnusable { named: named.to_owned() };
    if token.is_some() && (offset.is_some() || limit.is_some()) {
        return Err(unusable(CONTINUATION_TOKEN_OPTION));
    }
    if let Some(stated) = token {
        return ResultWindow::continuation(stated.clone())
            .map(Some)
            .map_err(|_| unusable(CONTINUATION_TOKEN_OPTION));
    }
    let (Some(offset), Some(limit)) = (offset, limit) else {
        if offset.is_some() || limit.is_some() {
            return Err(unusable(OFFSET_OPTION));
        }
        return Ok(None);
    };
    let offset: u64 = offset.parse().map_err(|_| unusable(OFFSET_OPTION))?;
    let limit: u64 = limit.parse().map_err(|_| unusable(LIMIT_OPTION))?;
    ResultWindow::initial(offset, limit).map(Some).map_err(|_| unusable(LIMIT_OPTION))
}
