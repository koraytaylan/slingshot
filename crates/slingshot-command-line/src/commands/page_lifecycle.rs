//! Changing, moving, removing, and walking pages and components.
//!
//! Eleven commands that share one habit: `--path` names what the command acts on.
//! A page for the page commands, a component resource for the component ones, an
//! anchor for the listings. A caller who has learned that once has learned it for
//! all nine, and no command here asks for the same subject under a second name.
//! The three listings differ only in what they admit: every child, the children
//! of one named primary type, or the pages.

use slingshot_domain::command::catalog::Command;
use slingshot_domain::command::delete_component::DeleteComponentCommand;
use slingshot_domain::command::delete_page::DeletePageCommand;
use slingshot_domain::command::list_child_nodes::{
    ListChildNodesByTypeCommand, ListChildNodesCommand,
};
use slingshot_domain::command::list_child_pages::ListChildPagesCommand;
use slingshot_domain::command::list_component_definitions::ListComponentDefinitionsCommand;
use slingshot_domain::command::list_components::ListComponentsCommand;
use slingshot_domain::command::list_content_fragment_models::ListContentFragmentModelsCommand;
use slingshot_domain::command::list_content_fragments::ListContentFragmentsCommand;
use slingshot_domain::command::list_experience_fragments::ListExperienceFragmentsCommand;
use slingshot_domain::command::list_page_templates::ListPageTemplatesCommand;
use slingshot_domain::command::move_page::MovePageCommand;
use slingshot_domain::command::reorder_component::{ComponentPlacement, ReorderComponentCommand};
use slingshot_domain::command::repository_path::{ComponentName, PrimaryNodeTypeName};
use slingshot_domain::command::update_component::UpdateComponentCommand;
use slingshot_domain::command::update_page::UpdatePageCommand;

use crate::commands::content::{RequestRefusal, require_key, required};
use crate::commands::operational_values::{
    flag, path, reference_policy, removed_property_names, title, unusable,
};
use crate::commands::page_mutation::properties;
use crate::commands::path_query::window;
use crate::invocation::{
    ADJUST_REFERENCES_OPTION, DESTINATION_PATH_OPTION, Invocation, NODE_TYPE_OPTION, PATH_OPTION,
    PLACEMENT_OPTION, SIBLING_OPTION,
};

/// The wire name of the page update.
pub const UPDATE_PAGE: &str = "update_page";

/// The wire name of the page deletion.
pub const DELETE_PAGE: &str = "delete_page";

/// The wire name of the page move.
pub const MOVE_PAGE: &str = "move_page";

/// The wire name of the child listing.
pub const LIST_CHILD_PAGES: &str = "list_child_pages";

/// The wire name of the component definition listing.
pub const LIST_COMPONENT_DEFINITIONS: &str = "list_component_definitions";

/// The wire name of the component instance listing.
pub const LIST_COMPONENTS: &str = "list_components";

/// The wire name of the content fragment model listing.
pub const LIST_CONTENT_FRAGMENT_MODELS: &str = "list_content_fragment_models";

/// The wire name of the content fragment listing.
pub const LIST_CONTENT_FRAGMENTS: &str = "list_content_fragments";

/// The wire name of the experience fragment listing.
pub const LIST_EXPERIENCE_FRAGMENTS: &str = "list_experience_fragments";

/// The wire name of the page template listing.
pub const LIST_PAGE_TEMPLATES: &str = "list_page_templates";

/// The wire name of the untyped child listing.
pub const LIST_CHILD_NODES: &str = "list_child_nodes";

/// The wire name of the typed child listing.
pub const LIST_CHILD_NODES_BY_TYPE: &str = "list_child_nodes_by_type";

/// The wire name of the component update.
pub const UPDATE_COMPONENT: &str = "update_component";

/// The wire name of the component deletion.
pub const DELETE_COMPONENT: &str = "delete_component";

/// The wire name of the component reordering.
pub const REORDER_COMPONENT: &str = "reorder_component";

/// The placement spelling that puts a component after every sibling.
pub const PLACEMENT_LAST: &str = "last";

/// The placement spelling that puts a component in front of one sibling.
pub const PLACEMENT_BEFORE: &str = "before";

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
    build_page(invocation).unwrap_or_else(|| build_component(invocation))
}

/// Every command this family builds.
const NAMES: &[&str] = &[
    UPDATE_PAGE,
    DELETE_PAGE,
    MOVE_PAGE,
    LIST_CHILD_NODES,
    LIST_CHILD_NODES_BY_TYPE,
    LIST_CHILD_PAGES,
    LIST_COMPONENT_DEFINITIONS,
    LIST_COMPONENTS,
    LIST_CONTENT_FRAGMENT_MODELS,
    LIST_CONTENT_FRAGMENTS,
    LIST_EXPERIENCE_FRAGMENTS,
    LIST_PAGE_TEMPLATES,
    UPDATE_COMPONENT,
    DELETE_COMPONENT,
    REORDER_COMPONENT,
];

/// Returns the page command one invocation describes, when it describes one.
fn build_page(invocation: &Invocation) -> Option<Result<Command, RequestRefusal>> {
    let built = match invocation.verb.as_str() {
        UPDATE_PAGE => update_page(invocation),
        DELETE_PAGE => delete_page(invocation),
        MOVE_PAGE => move_page(invocation),
        LIST_CHILD_NODES => list_child_nodes(invocation),
        LIST_CHILD_NODES_BY_TYPE => list_child_nodes_by_type(invocation),
        LIST_CHILD_PAGES => list_child_pages(invocation),
        LIST_COMPONENT_DEFINITIONS => list_component_definitions(invocation),
        LIST_COMPONENTS => list_components(invocation),
        LIST_CONTENT_FRAGMENT_MODELS => list_content_fragment_models(invocation),
        LIST_CONTENT_FRAGMENTS => list_content_fragments(invocation),
        LIST_EXPERIENCE_FRAGMENTS => list_experience_fragments(invocation),
        LIST_PAGE_TEMPLATES => list_page_templates(invocation),
        _ => return None,
    };
    Some(built)
}

/// Returns the component command one invocation describes.
fn build_component(invocation: &Invocation) -> Result<Command, RequestRefusal> {
    match invocation.verb.as_str() {
        UPDATE_COMPONENT => update_component(invocation),
        DELETE_COMPONENT => Ok(Command::DeleteComponent(DeleteComponentCommand {
            component_path: path(invocation, PATH_OPTION)?,
        })),
        _ => reorder_component(invocation),
    }
}

/// Returns the page update one invocation describes.
fn update_page(invocation: &Invocation) -> Result<Command, RequestRefusal> {
    Ok(Command::UpdatePage(UpdatePageCommand {
        page_path: path(invocation, PATH_OPTION)?,
        properties: properties(invocation, &[])?,
        removed_property_names: removed_property_names(invocation)?,
        title: title(invocation)?,
    }))
}

/// Returns the page deletion one invocation describes.
fn delete_page(invocation: &Invocation) -> Result<Command, RequestRefusal> {
    Ok(Command::DeletePage(DeletePageCommand {
        page_path: path(invocation, PATH_OPTION)?,
        reference_policy: reference_policy(invocation)?,
    }))
}

/// Returns the page move one invocation describes.
fn move_page(invocation: &Invocation) -> Result<Command, RequestRefusal> {
    Ok(Command::MovePage(MovePageCommand {
        adjust_references: flag(invocation, ADJUST_REFERENCES_OPTION),
        destination_path: path(invocation, DESTINATION_PATH_OPTION)?,
        source_path: path(invocation, PATH_OPTION)?,
    }))
}

/// Returns the untyped child listing one invocation describes.
fn list_child_nodes(invocation: &Invocation) -> Result<Command, RequestRefusal> {
    Ok(Command::ListChildNodes(ListChildNodesCommand {
        result_window: window(invocation)?,
        root_path: path(invocation, PATH_OPTION)?,
    }))
}

/// Returns the typed child listing one invocation describes.
///
/// The type is required rather than defaulted: a caller who asked for children
/// by type and named no type asked an incomplete question, and answering it with
/// every child would be answering a different one.
fn list_child_nodes_by_type(invocation: &Invocation) -> Result<Command, RequestRefusal> {
    let primary_node_type = PrimaryNodeTypeName::parse(required(invocation, NODE_TYPE_OPTION)?)
        .map_err(|_| unusable(NODE_TYPE_OPTION))?;
    Ok(Command::ListChildNodesByType(ListChildNodesByTypeCommand {
        primary_node_type,
        result_window: window(invocation)?,
        root_path: path(invocation, PATH_OPTION)?,
    }))
}

/// Returns the child listing one invocation describes.
fn list_child_pages(invocation: &Invocation) -> Result<Command, RequestRefusal> {
    Ok(Command::ListChildPages(ListChildPagesCommand {
        result_window: window(invocation)?,
        root_path: path(invocation, PATH_OPTION)?,
    }))
}

/// Returns the component definition listing one invocation describes.
fn list_component_definitions(invocation: &Invocation) -> Result<Command, RequestRefusal> {
    Ok(Command::ListComponentDefinitions(ListComponentDefinitionsCommand {
        result_window: window(invocation)?,
        root_path: path(invocation, PATH_OPTION)?,
    }))
}

/// Returns the component instance listing one invocation describes.
fn list_components(invocation: &Invocation) -> Result<Command, RequestRefusal> {
    Ok(Command::ListComponents(ListComponentsCommand {
        result_window: window(invocation)?,
        root_path: path(invocation, PATH_OPTION)?,
    }))
}

/// Returns the content fragment model listing one invocation describes.
fn list_content_fragment_models(invocation: &Invocation) -> Result<Command, RequestRefusal> {
    Ok(Command::ListContentFragmentModels(ListContentFragmentModelsCommand {
        result_window: window(invocation)?,
        root_path: path(invocation, PATH_OPTION)?,
    }))
}

/// Returns the content fragment listing one invocation describes.
fn list_content_fragments(invocation: &Invocation) -> Result<Command, RequestRefusal> {
    Ok(Command::ListContentFragments(ListContentFragmentsCommand {
        result_window: window(invocation)?,
        root_path: path(invocation, PATH_OPTION)?,
    }))
}

/// Returns the experience fragment listing one invocation describes.
fn list_experience_fragments(invocation: &Invocation) -> Result<Command, RequestRefusal> {
    Ok(Command::ListExperienceFragments(ListExperienceFragmentsCommand {
        result_window: window(invocation)?,
        root_path: path(invocation, PATH_OPTION)?,
    }))
}

/// Returns the page template listing one invocation describes.
fn list_page_templates(invocation: &Invocation) -> Result<Command, RequestRefusal> {
    Ok(Command::ListPageTemplates(ListPageTemplatesCommand {
        result_window: window(invocation)?,
        root_path: path(invocation, PATH_OPTION)?,
    }))
}

/// Returns the component update one invocation describes.
fn update_component(invocation: &Invocation) -> Result<Command, RequestRefusal> {
    Ok(Command::UpdateComponent(UpdateComponentCommand {
        component_path: path(invocation, PATH_OPTION)?,
        properties: properties(invocation, &[])?,
        removed_property_names: removed_property_names(invocation)?,
    }))
}

/// Returns the component reordering one invocation describes.
fn reorder_component(invocation: &Invocation) -> Result<Command, RequestRefusal> {
    let placement = match required(invocation, PLACEMENT_OPTION)? {
        // A sibling beside `last` is refused rather than dropped. The document
        // form refuses it, and a command line that accepted it would leave a
        // caller believing the component went in front of something.
        PLACEMENT_LAST if invocation.arguments.contains_key(SIBLING_OPTION) => {
            return Err(unusable(SIBLING_OPTION));
        }
        PLACEMENT_LAST => ComponentPlacement::Last {},
        PLACEMENT_BEFORE => ComponentPlacement::Before {
            sibling_name: ComponentName::parse(required(invocation, SIBLING_OPTION)?)
                .map_err(|_| unusable(SIBLING_OPTION))?,
        },
        _ => return Err(unusable(PLACEMENT_OPTION)),
    };
    Ok(Command::ReorderComponent(ReorderComponentCommand {
        component_path: path(invocation, PATH_OPTION)?,
        placement,
    }))
}
