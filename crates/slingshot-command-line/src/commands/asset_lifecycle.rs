//! Making, describing, moving, removing, and looking inside assets.
//!
//! `--path` names what the command acts on, which is the parent for the two
//! creations and the asset itself for everything else - the same habit
//! `create_page` already set, where the path is where the new thing goes and
//! `--name` is what it is called.
//!
//! Creating an asset is the one command on this surface that carries bytes. They
//! arrive already encoded, in `--payload`, because a command line that read a
//! file here would be a second file-reading rule beside the one `--properties`
//! already has, and the bound that matters is on the encoded form either way.

use slingshot_domain::command::catalog::Command;
use slingshot_domain::command::create_asset::CreateAssetCommand;
use slingshot_domain::command::create_asset_folder::CreateAssetFolderCommand;
use slingshot_domain::command::delete_asset::DeleteAssetCommand;
use slingshot_domain::command::list_asset_renditions::ListAssetRenditionsCommand;
use slingshot_domain::command::move_asset::MoveAssetCommand;
use slingshot_domain::command::repository_path::RepositoryName;
use slingshot_domain::command::resource_mutation::InlineBinaryPayload;
use slingshot_domain::command::update_asset_metadata::UpdateAssetMetadataCommand;

use crate::commands::content::{RequestRefusal, require_key, required};
use crate::commands::operational_values::{
    flag, path, reference_policy, removed_property_names, title, unusable,
};
use crate::commands::page_mutation::properties;
use crate::commands::path_query::window;
use crate::invocation::{
    ADJUST_REFERENCES_OPTION, DESTINATION_PATH_OPTION, Invocation, MEDIA_TYPE_OPTION, NAME_OPTION,
    PATH_OPTION, PAYLOAD_OPTION,
};

/// The wire name of the folder creation.
pub const CREATE_ASSET_FOLDER: &str = "create_asset_folder";

/// The wire name of the asset creation.
pub const CREATE_ASSET: &str = "create_asset";

/// The wire name of the metadata update.
pub const UPDATE_ASSET_METADATA: &str = "update_asset_metadata";

/// The wire name of the asset deletion.
pub const DELETE_ASSET: &str = "delete_asset";

/// The wire name of the asset move.
pub const MOVE_ASSET: &str = "move_asset";

/// The wire name of the rendition listing.
pub const LIST_ASSET_RENDITIONS: &str = "list_asset_renditions";

/// Every command this family builds.
const NAMES: &[&str] = &[
    CREATE_ASSET_FOLDER,
    CREATE_ASSET,
    UPDATE_ASSET_METADATA,
    DELETE_ASSET,
    MOVE_ASSET,
    LIST_ASSET_RENDITIONS,
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
        CREATE_ASSET_FOLDER => create_folder(invocation),
        CREATE_ASSET => create_asset(invocation),
        UPDATE_ASSET_METADATA => update_metadata(invocation),
        DELETE_ASSET => Ok(Command::DeleteAsset(DeleteAssetCommand {
            asset_path: path(invocation, PATH_OPTION)?,
            reference_policy: reference_policy(invocation)?,
        })),
        MOVE_ASSET => move_asset(invocation),
        _ => Ok(Command::ListAssetRenditions(ListAssetRenditionsCommand {
            asset_path: path(invocation, PATH_OPTION)?,
            result_window: window(invocation)?,
        })),
    }
}

/// Returns the folder creation one invocation describes.
fn create_folder(invocation: &Invocation) -> Result<Command, RequestRefusal> {
    Ok(Command::CreateAssetFolder(CreateAssetFolderCommand {
        name: name(invocation)?,
        parent_path: path(invocation, PATH_OPTION)?,
        title: title(invocation)?,
    }))
}

/// Returns the asset creation one invocation describes.
fn create_asset(invocation: &Invocation) -> Result<Command, RequestRefusal> {
    let payload = InlineBinaryPayload::new(
        required(invocation, MEDIA_TYPE_OPTION)?,
        required(invocation, PAYLOAD_OPTION)?,
    )
    .map_err(|_| unusable(PAYLOAD_OPTION))?;
    Ok(Command::CreateAsset(CreateAssetCommand {
        metadata: properties(invocation, &[])?,
        name: name(invocation)?,
        parent_path: path(invocation, PATH_OPTION)?,
        payload,
    }))
}

/// Returns the metadata update one invocation describes.
fn update_metadata(invocation: &Invocation) -> Result<Command, RequestRefusal> {
    Ok(Command::UpdateAssetMetadata(UpdateAssetMetadataCommand {
        asset_path: path(invocation, PATH_OPTION)?,
        properties: properties(invocation, &[])?,
        removed_property_names: removed_property_names(invocation)?,
    }))
}

/// Returns the asset move one invocation describes.
fn move_asset(invocation: &Invocation) -> Result<Command, RequestRefusal> {
    Ok(Command::MoveAsset(MoveAssetCommand {
        adjust_references: flag(invocation, ADJUST_REFERENCES_OPTION),
        destination_path: path(invocation, DESTINATION_PATH_OPTION)?,
        source_path: path(invocation, PATH_OPTION)?,
    }))
}

/// Returns the name a creation gives its subject.
fn name(invocation: &Invocation) -> Result<RepositoryName, RequestRefusal> {
    RepositoryName::parse(required(invocation, NAME_OPTION)?).map_err(|_| unusable(NAME_OPTION))
}
