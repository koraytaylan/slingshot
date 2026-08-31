//! Finding pages by template, phrase, or component.
//!
//! Three commands that share a root and a window and differ in what they look
//! for. The window handling is the path query's, because a caller who learns it
//! once should not learn it again per command, and two implementations of one
//! rule eventually disagree about whether a token may sit beside an offset.
//!
//! # A phrase is passed as it was typed
//!
//! Not trimmed, not normalized, not case-folded. What a caller typed is what is
//! searched for, and the domain refuses a phrase with leading or trailing
//! whitespace rather than quietly removing it - because removing it would mean
//! searching for something the caller did not ask for and reporting matches as
//! though they had.

use slingshot_domain::command::catalog::Command;
use slingshot_domain::command::component_resource_type::ComponentResourceType;
use slingshot_domain::command::find_pages_by_template::FindPagesByTemplateCommand;
use slingshot_domain::command::find_pages_containing_phrase::{
    FindPagesContainingPhraseCommand, SearchPhrase,
};
use slingshot_domain::command::find_pages_using_components::{
    ComponentMatchMode, FindPagesUsingComponentsCommand, RequestedComponentResourceTypes,
};
use slingshot_domain::command::repository_path::RepositoryPath;

use crate::commands::content::{RequestRefusal, require_key, required};
use crate::commands::package::LIST_SEPARATOR;
use crate::commands::path_query::window;
use crate::invocation::{
    Invocation, MATCH_ALL_OPTION, PATH_OPTION, PHRASE_OPTION, RESOURCE_TYPES_OPTION,
    TEMPLATE_OPTION,
};

/// The wire name of the template search.
pub const FIND_BY_TEMPLATE: &str = "find_pages_by_template";

/// The wire name of the phrase search.
pub const FIND_CONTAINING_PHRASE: &str = "find_pages_containing_phrase";

/// The wire name of the component search.
pub const FIND_USING_COMPONENTS: &str = "find_pages_using_components";

/// Returns the typed request one invocation describes.
///
/// # Errors
///
/// Returns [`RequestRefusal`] naming the first thing that is wrong.
pub fn build(invocation: &Invocation) -> Result<Command, RequestRefusal> {
    // The verb is answered before any option is read. A family that reads an
    // option first turns "this is not my command" into "you forgot --path", and
    // the assembler stops at the first refusal that is not `AnotherCommand` - so
    // one family reading early would hide every family declared after it.
    if !NAMES.contains(&invocation.verb.as_str()) {
        return Err(RequestRefusal::AnotherCommand { named: invocation.verb.clone() });
    }
    require_key(invocation)?;
    let root_path = RepositoryPath::parse(required(invocation, PATH_OPTION)?)
        .map_err(|_| RequestRefusal::ValueUnusable { named: PATH_OPTION.to_owned() })?;
    let result_window = window(invocation)?;
    match invocation.verb.as_str() {
        FIND_BY_TEMPLATE => {
            let template_path = RepositoryPath::parse(required(invocation, TEMPLATE_OPTION)?)
                .map_err(|_| RequestRefusal::ValueUnusable { named: TEMPLATE_OPTION.to_owned() })?;
            Ok(Command::FindPagesByTemplate(FindPagesByTemplateCommand {
                result_window,
                root_path,
                template_path,
            }))
        }
        FIND_CONTAINING_PHRASE => {
            let phrase = SearchPhrase::new(required(invocation, PHRASE_OPTION)?)
                .map_err(|_| RequestRefusal::ValueUnusable { named: PHRASE_OPTION.to_owned() })?;
            Ok(Command::FindPagesContainingPhrase(FindPagesContainingPhraseCommand {
                phrase,
                result_window,
                root_path,
            }))
        }
        FIND_USING_COMPONENTS => {
            Ok(Command::FindPagesUsingComponents(FindPagesUsingComponentsCommand {
                match_mode: match_mode(invocation),
                resource_types: resource_types(invocation)?,
                result_window,
                root_path,
            }))
        }
        named => Err(RequestRefusal::AnotherCommand { named: named.to_owned() }),
    }
}

/// Every command this family builds.
const NAMES: &[&str] = &[FIND_BY_TEMPLATE, FIND_CONTAINING_PHRASE, FIND_USING_COMPONENTS];

/// Returns how many of the named components a page must use.
fn match_mode(invocation: &Invocation) -> ComponentMatchMode {
    if invocation.arguments.contains_key(MATCH_ALL_OPTION) {
        ComponentMatchMode::All
    } else {
        ComponentMatchMode::Any
    }
}

/// Returns the component types one invocation names.
fn resource_types(
    invocation: &Invocation,
) -> Result<RequestedComponentResourceTypes, RequestRefusal> {
    let unusable = || RequestRefusal::ValueUnusable { named: RESOURCE_TYPES_OPTION.to_owned() };
    let stated = required(invocation, RESOURCE_TYPES_OPTION)?;
    let types = stated
        .split(LIST_SEPARATOR)
        .map(|part| ComponentResourceType::parse(part).map_err(|_| unusable()))
        .collect::<Result<Vec<ComponentResourceType>, RequestRefusal>>()?;
    RequestedComponentResourceTypes::new(types).map_err(|_| unusable())
}
