//! Creating a page and adding a component to one.
//!
//! The two commands that change content. Both take a property document from a
//! file, and both refuse the property they set themselves: a document that
//! could redefine a page's title or a component's resource type would make two
//! parts of one request disagree, with nothing to say which won.
//!
//! Both are write-classified and neither is intrinsically idempotent, so both
//! demand a caller key. The key is checked before the property file is opened,
//! not after: a caller who forgot it should not have their file read, and a
//! read that happened anyway would be a side effect of an invocation that was
//! never going to be sent.

use slingshot_domain::command::add_component::{
    AddComponentCommand, COMPONENT_RESOURCE_TYPE_PROPERTY, ContentRootMarker, PageContentParent,
};
use slingshot_domain::command::catalog::Command;
use slingshot_domain::command::component_resource_type::ComponentResourceType;
use slingshot_domain::command::create_page::{
    CreatePageCommand, MutationProperties, PAGE_TITLE_PROPERTY,
};
use slingshot_domain::command::repository_path::{
    ComponentName, PageName, RepositoryPath, RepositoryRelativePath,
};

use crate::commands::content::{RequestRefusal, require_key, required};
use crate::invocation::{
    COMPONENT_PARENT_OPTION, Invocation, NAME_OPTION, PATH_OPTION, PROPERTIES_OPTION,
    RESOURCE_TYPE_OPTION, TEMPLATE_OPTION, TITLE_OPTION,
};
use crate::property_document::read;

/// The wire name of the page creation.
pub const CREATE_PAGE: &str = "create_page";

/// The wire name of the component addition.
pub const ADD_COMPONENT: &str = "add_component";

/// The spelling that names a page's content resource itself.
pub const CONTENT_ROOT: &str = "content-root";

/// Returns the typed request one invocation describes.
///
/// # Errors
///
/// Returns [`RequestRefusal`] naming the first thing that is wrong. The key is
/// checked before the property file is opened, so a caller who forgot it does
/// not have their file read on the way to being told.
pub fn build(invocation: &Invocation) -> Result<Command, RequestRefusal> {
    require_key(invocation)?;
    match invocation.verb.as_str() {
        CREATE_PAGE => build_page(invocation),
        ADD_COMPONENT => build_component(invocation),
        named => Err(RequestRefusal::AnotherCommand { named: named.to_owned() }),
    }
}

/// Returns the page creation one invocation describes.
fn build_page(invocation: &Invocation) -> Result<Command, RequestRefusal> {
    let parent_path = RepositoryPath::parse(required(invocation, PATH_OPTION)?)
        .map_err(|_| RequestRefusal::ValueUnusable { named: PATH_OPTION.to_owned() })?;
    let page_name = PageName::parse(required(invocation, NAME_OPTION)?)
        .map_err(|_| RequestRefusal::ValueUnusable { named: NAME_OPTION.to_owned() })?;
    let template_path = RepositoryPath::parse(required(invocation, TEMPLATE_OPTION)?)
        .map_err(|_| RequestRefusal::ValueUnusable { named: TEMPLATE_OPTION.to_owned() })?;
    let title = required(invocation, TITLE_OPTION)?.to_owned();
    Ok(Command::CreatePage(CreatePageCommand {
        initial_properties: properties(invocation, &[PAGE_TITLE_PROPERTY])?,
        page_name,
        parent_path,
        template_path,
        title,
    }))
}

/// Returns the component addition one invocation describes.
fn build_component(invocation: &Invocation) -> Result<Command, RequestRefusal> {
    let page_path = RepositoryPath::parse(required(invocation, PATH_OPTION)?)
        .map_err(|_| RequestRefusal::ValueUnusable { named: PATH_OPTION.to_owned() })?;
    let component_name = ComponentName::parse(required(invocation, NAME_OPTION)?)
        .map_err(|_| RequestRefusal::ValueUnusable { named: NAME_OPTION.to_owned() })?;
    let resource_type = ComponentResourceType::parse(required(invocation, RESOURCE_TYPE_OPTION)?)
        .map_err(|_| RequestRefusal::ValueUnusable {
        named: RESOURCE_TYPE_OPTION.to_owned(),
    })?;
    Ok(Command::AddComponent(AddComponentCommand {
        component_name,
        content_parent: content_parent(invocation)?,
        page_path,
        properties: properties(invocation, &[COMPONENT_RESOURCE_TYPE_PROPERTY])?,
        resource_type,
    }))
}

/// Returns where under the page's content resource a component goes.
///
/// The content resource itself unless the caller named a descendant, because
/// most components go at the top and asking for that every time would be
/// ceremony.
fn content_parent(invocation: &Invocation) -> Result<PageContentParent, RequestRefusal> {
    let Some(stated) = invocation.arguments.get(COMPONENT_PARENT_OPTION) else {
        return Ok(PageContentParent::ContentRoot(ContentRootMarker::ContentRoot));
    };
    if stated == CONTENT_ROOT {
        return Ok(PageContentParent::ContentRoot(ContentRootMarker::ContentRoot));
    }
    RepositoryRelativePath::parse(stated)
        .map(PageContentParent::Descendant)
        .map_err(|_| RequestRefusal::ValueUnusable { named: COMPONENT_PARENT_OPTION.to_owned() })
}

/// Returns the properties one invocation's document holds, when it names one.
pub(crate) fn properties(
    invocation: &Invocation,
    reserved: &[&str],
) -> Result<Option<MutationProperties>, RequestRefusal> {
    let Some(stated) = invocation.arguments.get(PROPERTIES_OPTION) else {
        return Ok(None);
    };
    let unusable = || RequestRefusal::ValueUnusable { named: PROPERTIES_OPTION.to_owned() };
    let values = read(std::path::Path::new(stated)).map_err(|_| unusable())?;
    MutationProperties::new(values, reserved).map(Some).map_err(|_| unusable())
}
