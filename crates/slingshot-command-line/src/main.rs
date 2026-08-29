//! Process entry point for the `slingshot` product executable.
//!
//! The entry point parses the invocation, resolves its own absolute path so a
//! start can create a child from the same executable, runs the command on the
//! asynchronous runtime, and maps the outcome onto a process exit status.
//! Diagnostics go to the diagnostic stream; the result stream carries the
//! structured result and nothing else.

use std::io;
use std::process::ExitCode;

use clap::Parser;
use slingshot_command_line::command_line::{self, ProductArguments};

fn main() -> ExitCode {
    let arguments = ProductArguments::parse();
    let executable = match std::env::current_exe() {
        Ok(path) => path,
        Err(failure) => {
            eprintln!("slingshot: the running executable could not be resolved: {failure}");
            return ExitCode::from(command_line::EXIT_RUNTIME_UNUSABLE);
        }
    };
    let runtime = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
        Ok(runtime) => runtime,
        Err(failure) => {
            eprintln!("slingshot: the runtime could not be started: {failure}");
            return ExitCode::from(command_line::EXIT_RUNTIME_UNUSABLE);
        }
    };
    let mut standard_output = io::stdout().lock();
    match runtime.block_on(command_line::run(arguments, &executable, &mut standard_output)) {
        Ok(status) => ExitCode::from(status),
        Err(failure) => {
            eprintln!("slingshot: {failure}");
            ExitCode::from(failure.exit_status())
        }
    }
}
