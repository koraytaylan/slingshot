//! Making, editing, and removing experience fragments.
//!
//! The update addresses a variation directly, because that is where an
//! experience fragment's content is: `--path` names the variation for the
//! update and the fragment for the deletion, and the creation names its parent
//! and gets both addresses back.

use slingshot_domain::command::catalog::Command;
use slingshot_domain::command::content_fragment_element::ContentFragmentVariationName;
use slingshot_domain::command::create_experience_fragment::CreateExperienceFragmentCommand;
use slingshot_domain::command::delete_experience_fragment::DeleteExperienceFragmentCommand;
use slingshot_domain::command::find_pages_containing_phrase::PageTitle;
use slingshot_domain::command::repository_path::RepositoryName;
use slingshot_domain::command::update_experience_fragment::UpdateExperienceFragmentCommand;

use crate::commands::content::{RequestRefusal, require_key, required};
use crate::commands::operational_values::{
    optional_text, path, reference_policy, removed_property_names, unusable,
};
use crate::commands::page_mutation::properties;
use crate::invocation::{
    Invocation, NAME_OPTION, PATH_OPTION, TEMPLATE_OPTION, TITLE_OPTION, VARIATION_OPTION,
};

/// The wire name of the fragment creation.
pub const CREATE_EXPERIENCE_FRAGMENT: &str = "create_experience_fragment";

/// The wire name of the variation update.
pub const UPDATE_EXPERIENCE_FRAGMENT: &str = "update_experience_fragment";

/// The wire name of the fragment deletion.
pub const DELETE_EXPERIENCE_FRAGMENT: &str = "delete_experience_fragment";

/// Every command this family builds.
const NAMES: &[&str] =
    &[CREATE_EXPERIENCE_FRAGMENT, UPDATE_EXPERIENCE_FRAGMENT, DELETE_EXPERIENCE_FRAGMENT];

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
        CREATE_EXPERIENCE_FRAGMENT => create(invocation),
        UPDATE_EXPERIENCE_FRAGMENT => update(invocation),
        _ => Ok(Command::DeleteExperienceFragment(DeleteExperienceFragmentCommand {
            fragment_path: path(invocation, PATH_OPTION)?,
            reference_policy: reference_policy(invocation)?,
        })),
    }
}

/// Returns the fragment creation one invocation describes.
fn create(invocation: &Invocation) -> Result<Command, RequestRefusal> {
    let variation_name =
        ContentFragmentVariationName::parse(required(invocation, VARIATION_OPTION)?)
            .map_err(|_| unusable(VARIATION_OPTION))?;
    Ok(Command::CreateExperienceFragment(CreateExperienceFragmentCommand {
        name: RepositoryName::parse(required(invocation, NAME_OPTION)?)
            .map_err(|_| unusable(NAME_OPTION))?,
        parent_path: path(invocation, PATH_OPTION)?,
        template_path: path(invocation, TEMPLATE_OPTION)?,
        title: title(invocation)?,
        variation_name,
    }))
}

/// Returns the variation update one invocation describes.
fn update(invocation: &Invocation) -> Result<Command, RequestRefusal> {
    Ok(Command::UpdateExperienceFragment(UpdateExperienceFragmentCommand {
        properties: properties(invocation, &[])?,
        removed_property_names: removed_property_names(invocation)?,
        title: title(invocation)?,
        variation_path: path(invocation, PATH_OPTION)?,
    }))
}

/// Returns the title one invocation records, when it records one.
fn title(invocation: &Invocation) -> Result<Option<PageTitle>, RequestRefusal> {
    optional_text(invocation, TITLE_OPTION)
        .map(|stated| PageTitle::new(stated).map_err(|_| unusable(TITLE_OPTION)))
        .transpose()
}
