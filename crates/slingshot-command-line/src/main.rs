//! Process entry point for the `slingshot` product executable.
//!
//! The entry point separates the executable path from the arguments, delegates
//! to the command-line library, and maps the outcome onto a process exit code.

use std::io;
use std::process::ExitCode;

/// Number of leading process arguments that carry the executable path.
const EXECUTABLE_PATH_ARGUMENT_COUNT: usize = 1;

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(EXECUTABLE_PATH_ARGUMENT_COUNT).collect();
    let mut standard_output = io::stdout().lock();
    match slingshot_command_line::execute(&arguments, &mut standard_output) {
        Ok(()) => ExitCode::SUCCESS,
        Err(failure) => {
            eprintln!("slingshot: {failure}");
            ExitCode::FAILURE
        }
    }
}
