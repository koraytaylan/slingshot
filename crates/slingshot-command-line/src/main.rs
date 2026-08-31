//! Process entry point for the `slingshot` product executable.
//!
//! Thin on purpose. It hands the argument vector, this executable's own path,
//! and the two streams to the dispatcher, and turns the exit it returns into a
//! process status. Every decision, every effect, and every byte written belongs
//! to something that can be driven without a process.

use std::io;
use std::process::ExitCode;

use slingshot_command_line::command_line;

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let executable = match std::env::current_exe() {
        Ok(path) => path,
        Err(failure) => {
            eprintln!("slingshot: the running executable could not be resolved: {failure}");
            return ExitCode::from(command_line::EXIT_RUNTIME_UNUSABLE);
        }
    };
    let mut standard_output = io::stdout().lock();
    let mut standard_error = io::stderr().lock();
    let exit =
        command_line::run(&arguments, &executable, &mut standard_output, &mut standard_error);
    ExitCode::from(u8::try_from(exit).unwrap_or(command_line::EXIT_RUNTIME_UNUSABLE))
}
