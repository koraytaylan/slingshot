//! Process entry point for the repository-command executable.
//!
//! The entry point separates the executable path from the arguments, resolves
//! the working directory, dispatches one repository command by name, and maps
//! the outcome onto a process exit code. Every command body lives in the
//! library, so a command stays testable without spawning a process.

use std::io::{self, Write};
use std::path::Path;
use std::process::ExitCode;

use slingshot_development::RepositoryCommandFailure;

/// Number of leading process arguments that carry the executable path.
const EXECUTABLE_PATH_ARGUMENT_COUNT: usize = 1;

/// Name of the command that emits resolved Cargo workspace metadata.
const WORKSPACE_METADATA_COMMAND: &str = "workspace-metadata";

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
        _ => Err(RepositoryCommandFailure::UnknownCommand(requested.clone())),
    }
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
