//! Building and downloading a content package.
//!
//! Roots, a name, and two ordered filter lists. Order is preserved exactly as
//! the caller gave it, because a package filter is read in order and reordering
//! the words would change which subtrees survive - quietly, and only for the
//! inputs where it mattered.
//!
//! What the expressions mean is the operation's business. This surface parses
//! them far enough to know they are expressions and no further, so a caller who
//! writes a pattern this build has never seen gets the agent's answer rather
//! than a local guess about it.

use slingshot_domain::command::catalog::Command;
use slingshot_domain::command::download_content_package::{
    DownloadContentPackageCommand, PackageName, PackageRoots, SelectionFilters,
    maximum_package_exclusion_expressions, maximum_package_inclusion_expressions,
};
use slingshot_domain::command::package_selection::PackagePathSelectionExpression;
use slingshot_domain::command::repository_path::RepositoryPath;

use crate::commands::content::{RequestRefusal, require_key, required};
use crate::invocation::Invocation;

/// The wire name of the command this family exposes.
pub const DOWNLOAD_PACKAGE: &str = "download_content_package";

/// The option naming the stem the produced file is named from.
pub const PACKAGE_NAME_OPTION: &str = "--package-name";

/// The option naming the subtrees to package, separated by commas.
pub const ROOTS_OPTION: &str = "--roots";

/// The option naming the subtrees to admit, in order.
pub const INCLUDE_OPTION: &str = "--include";

/// The option naming the subtrees to remove, in order.
pub const EXCLUDE_OPTION: &str = "--exclude";

/// The separator between the members of a list-valued option.
pub const LIST_SEPARATOR: char = ',';

/// Returns the typed request one invocation describes.
///
/// # Errors
///
/// Returns [`RequestRefusal`] naming the first thing that is wrong.
pub fn build(invocation: &Invocation) -> Result<Command, RequestRefusal> {
    if invocation.verb != DOWNLOAD_PACKAGE {
        return Err(RequestRefusal::AnotherCommand { named: invocation.verb.clone() });
    }
    require_key(invocation)?;
    let package_name = PackageName::new(required(invocation, PACKAGE_NAME_OPTION)?)
        .map_err(|_| RequestRefusal::ValueUnusable { named: PACKAGE_NAME_OPTION.to_owned() })?;
    let roots = parse_roots(required(invocation, ROOTS_OPTION)?)?;
    Ok(Command::DownloadContentPackage(DownloadContentPackageCommand {
        exclusion_filters: parse_filters(
            invocation,
            EXCLUDE_OPTION,
            maximum_package_exclusion_expressions(),
        )?,
        inclusion_filters: parse_filters(
            invocation,
            INCLUDE_OPTION,
            maximum_package_inclusion_expressions(),
        )?,
        package_name,
        roots,
    }))
}

/// Returns the roots one comma-separated value names.
fn parse_roots(stated: &str) -> Result<PackageRoots, RequestRefusal> {
    let unusable = || RequestRefusal::ValueUnusable { named: ROOTS_OPTION.to_owned() };
    let paths = stated
        .split(LIST_SEPARATOR)
        .map(|part| RepositoryPath::parse(part).map_err(|_| unusable()))
        .collect::<Result<Vec<RepositoryPath>, RequestRefusal>>()?;
    PackageRoots::new(paths).map_err(|_| unusable())
}

/// Returns the filters one option names, in the order they were given.
fn parse_filters(
    invocation: &Invocation,
    named: &str,
    maximum: u64,
) -> Result<Option<SelectionFilters>, RequestRefusal> {
    let Some(stated) = invocation.arguments.get(named) else {
        return Ok(None);
    };
    let unusable = || RequestRefusal::ValueUnusable { named: named.to_owned() };
    let expressions = stated
        .split(LIST_SEPARATOR)
        .map(|part| PackagePathSelectionExpression::parse(part).map_err(|_| unusable()))
        .collect::<Result<Vec<PackagePathSelectionExpression>, RequestRefusal>>()?;
    SelectionFilters::new(expressions, maximum).map(Some).map_err(|_| unusable())
}
