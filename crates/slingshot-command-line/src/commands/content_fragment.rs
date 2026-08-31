//! Making, reading, editing, and removing content fragments.
//!
//! `--path` names the parent for the creation and the fragment for everything
//! else; `--variation` names which variation is read or written, and its absence
//! means the master. `--elements` carries one JSON document, because the domain
//! already declares exactly what an element document is and a second grammar
//! here would be a second thing to disagree with it.

use slingshot_domain::command::catalog::Command;
use slingshot_domain::command::content_fragment_element::{
    ContentFragmentElementValues, ContentFragmentVariationName,
};
use slingshot_domain::command::create_content_fragment::CreateContentFragmentCommand;
use slingshot_domain::command::delete_content_fragment::DeleteContentFragmentCommand;
use slingshot_domain::command::read_content_fragment::ReadContentFragmentCommand;
use slingshot_domain::command::repository_path::RepositoryName;
use slingshot_domain::command::update_content_fragment::UpdateContentFragmentCommand;

use crate::commands::content::{RequestRefusal, require_key, required};
use crate::commands::operational_values::{
    optional_document, optional_text, path, reference_policy, title, unusable,
};
use crate::invocation::{
    ELEMENTS_OPTION, Invocation, MODEL_OPTION, NAME_OPTION, PATH_OPTION, VARIATION_OPTION,
};

/// The wire name of the fragment creation.
pub const CREATE_CONTENT_FRAGMENT: &str = "create_content_fragment";

/// The wire name of the fragment read.
pub const READ_CONTENT_FRAGMENT: &str = "read_content_fragment";

/// The wire name of the fragment update.
pub const UPDATE_CONTENT_FRAGMENT: &str = "update_content_fragment";

/// The wire name of the fragment deletion.
pub const DELETE_CONTENT_FRAGMENT: &str = "delete_content_fragment";

/// Every command this family builds.
const NAMES: &[&str] = &[
    CREATE_CONTENT_FRAGMENT,
    READ_CONTENT_FRAGMENT,
    UPDATE_CONTENT_FRAGMENT,
    DELETE_CONTENT_FRAGMENT,
];

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
        CREATE_CONTENT_FRAGMENT => create(invocation),
        READ_CONTENT_FRAGMENT => Ok(Command::ReadContentFragment(ReadContentFragmentCommand {
            fragment_path: path(invocation, PATH_OPTION)?,
            variation_name: variation(invocation)?,
        })),
        UPDATE_CONTENT_FRAGMENT => update(invocation),
        _ => Ok(Command::DeleteContentFragment(DeleteContentFragmentCommand {
            fragment_path: path(invocation, PATH_OPTION)?,
            reference_policy: reference_policy(invocation)?,
        })),
    }
}

/// Returns the fragment creation one invocation describes.
fn create(invocation: &Invocation) -> Result<Command, RequestRefusal> {
    Ok(Command::CreateContentFragment(CreateContentFragmentCommand {
        elements: elements(invocation)?,
        model_path: path(invocation, MODEL_OPTION)?,
        name: RepositoryName::parse(required(invocation, NAME_OPTION)?)
            .map_err(|_| unusable(NAME_OPTION))?,
        parent_path: path(invocation, PATH_OPTION)?,
        title: title(invocation)?,
    }))
}

/// Returns the fragment update one invocation describes.
fn update(invocation: &Invocation) -> Result<Command, RequestRefusal> {
    Ok(Command::UpdateContentFragment(UpdateContentFragmentCommand {
        elements: elements(invocation)?,
        fragment_path: path(invocation, PATH_OPTION)?,
        title: title(invocation)?,
        variation_name: variation(invocation)?,
    }))
}

/// Returns the element document one invocation carries, when it carries one.
fn elements(
    invocation: &Invocation,
) -> Result<Option<ContentFragmentElementValues>, RequestRefusal> {
    optional_document(invocation, ELEMENTS_OPTION)
}

/// Returns the variation one invocation names, when it names one.
fn variation(
    invocation: &Invocation,
) -> Result<Option<ContentFragmentVariationName>, RequestRefusal> {
    optional_text(invocation, VARIATION_OPTION)
        .map(|stated| {
            ContentFragmentVariationName::parse(&stated).map_err(|_| unusable(VARIATION_OPTION))
        })
        .transpose()
}
