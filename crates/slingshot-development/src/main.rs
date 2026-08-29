//! Process entry point for the repository-command executable.
//!
//! The entry point separates the executable path from the arguments, resolves
//! the working directory, dispatches one repository command by name, and maps
//! the outcome onto a process exit code. Every command body lives in the
//! library, so a command stays testable without spawning a process.

use std::io::{self, Write};
use std::path::Path;
use std::process::ExitCode;

use slingshot_development::{RepositoryCommandFailure, dependency_direction, source_policy};

/// Number of leading process arguments that carry the executable path.
const EXECUTABLE_PATH_ARGUMENT_COUNT: usize = 1;

/// Name of the command that emits resolved Cargo workspace metadata.
const WORKSPACE_METADATA_COMMAND: &str = "workspace-metadata";

/// Name of the command that checks the workspace dependency direction.
const DEPENDENCY_DIRECTION_COMMAND: &str = "dependency-direction";

/// Name of the command that checks every repository source policy rule.
const SOURCE_POLICY_COMMAND: &str = "source-policy";

/// Runs the repository command named by the first argument.
fn dispatch(
    arguments: &[String],
    working_directory: &Path,
    output: &mut dyn Write,
) -> Result<(), RepositoryCommandFailure> {
    let requested = arguments.first().ok_or(RepositoryCommandFailure::MissingCommand)?;
    match requested.as_str() {
        WORKSPACE_METADATA_COMMAND => {
            let workspace_root = slingshot_development::locate_workspace_root(working_directory)?;
            slingshot_development::emit_workspace_metadata(&workspace_root, output)
        }
        DEPENDENCY_DIRECTION_COMMAND => check_dependency_direction(working_directory, output),
        SOURCE_POLICY_COMMAND => check_source_policy(working_directory, output),
        _ => Err(RepositoryCommandFailure::UnknownCommand(requested.clone())),
    }
}

/// Reads the workspace metadata and reports every forbidden dependency edge.
fn check_dependency_direction(
    working_directory: &Path,
    output: &mut dyn Write,
) -> Result<(), RepositoryCommandFailure> {
    let workspace_root = slingshot_development::locate_workspace_root(working_directory)?;
    let mut metadata = Vec::new();
    slingshot_development::emit_workspace_metadata(&workspace_root, &mut metadata)?;
    let text =
        String::from_utf8(metadata).map_err(|failure| RepositoryCommandFailure::ToolFailed {
            program: WORKSPACE_METADATA_COMMAND.to_owned(),
            reason: failure.to_string(),
        })?;
    let graph = dependency_direction::read_graph(&text).map_err(|failure| {
        RepositoryCommandFailure::ToolFailed {
            program: DEPENDENCY_DIRECTION_COMMAND.to_owned(),
            reason: failure.to_string(),
        }
    })?;
    let violations = dependency_direction::evaluate(&graph);
    if violations.is_empty() {
        return writeln!(
            output,
            "{} local edges follow the dependency contract",
            graph.edges.len()
        )
        .map_err(|failure| RepositoryCommandFailure::OutputUnavailable(failure.to_string()));
    }
    for violation in &violations {
        writeln!(output, "{violation}")
            .map_err(|failure| RepositoryCommandFailure::OutputUnavailable(failure.to_string()))?;
    }
    Err(RepositoryCommandFailure::ToolFailed {
        program: DEPENDENCY_DIRECTION_COMMAND.to_owned(),
        reason: format!("{} forbidden local dependency edges", violations.len()),
    })
}

/// Reads the repository and reports every source policy rule it breaks.
fn check_source_policy(
    working_directory: &Path,
    output: &mut dyn Write,
) -> Result<(), RepositoryCommandFailure> {
    let workspace_root = slingshot_development::locate_workspace_root(working_directory)?;
    let violations = source_policy::check_repository(&workspace_root).map_err(|failure| {
        RepositoryCommandFailure::ToolFailed {
            program: SOURCE_POLICY_COMMAND.to_owned(),
            reason: failure.to_string(),
        }
    })?;
    for violation in &violations {
        writeln!(output, "{violation}")
            .map_err(|failure| RepositoryCommandFailure::OutputUnavailable(failure.to_string()))?;
    }
    if violations.is_empty() {
        return writeln!(output, "the repository follows every source policy rule")
            .map_err(|failure| RepositoryCommandFailure::OutputUnavailable(failure.to_string()));
    }
    Err(RepositoryCommandFailure::ToolFailed {
        program: SOURCE_POLICY_COMMAND.to_owned(),
        reason: format!("{} source policy violations", violations.len()),
    })
}

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(EXECUTABLE_PATH_ARGUMENT_COUNT).collect();
    let working_directory = match std::env::current_dir() {
        Ok(directory) => directory,
        Err(failure) => {
            eprintln!("slingshot-development: the working directory is unavailable: {failure}");
            return ExitCode::FAILURE;
        }
    };
    let mut standard_output = io::stdout().lock();
    match dispatch(&arguments, &working_directory, &mut standard_output) {
        Ok(()) => ExitCode::SUCCESS,
        Err(failure) => {
            eprintln!("slingshot-development: {failure}");
            ExitCode::FAILURE
        }
    }
}
