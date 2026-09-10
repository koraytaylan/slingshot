//! Process entry point for the `slingshot` product executable.
//!
//! Thin on purpose. It hands the argument vector, this executable's own path,
//! and the two streams to the dispatcher, and turns the exit it returns into a
//! process status. Every decision, every effect, and every byte written belongs
//! to something that can be driven without a process.

use std::io;
#[cfg(unix)]
use std::io::IsTerminal;
use std::process::ExitCode;

use slingshot_command_line::command_line;

/// Restores standard output's original status flags when the process returns.
#[cfg(unix)]
struct StandardOutputFlags(rustix::fs::OFlags);

#[cfg(unix)]
impl Drop for StandardOutputFlags {
    fn drop(&mut self) {
        let _ignored = rustix::fs::fcntl_setfl(std::io::stdout(), self.0);
    }
}

/// Makes standard output report a full pipe instead of blocking this process.
#[cfg(unix)]
fn standard_output_nonblocking() -> Option<StandardOutputFlags> {
    let output = std::io::stdout();
    if output.is_terminal() {
        return None;
    }
    let original = rustix::fs::fcntl_getfl(&output).ok()?;
    rustix::fs::fcntl_setfl(&output, original | rustix::fs::OFlags::NONBLOCK).ok()?;
    Some(StandardOutputFlags(original))
}

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let executable = match std::env::current_exe() {
        Ok(path) => path,
        Err(failure) => {
            eprintln!("slingshot: the running executable could not be resolved: {failure}");
            return ExitCode::from(command_line::EXIT_RUNTIME_UNUSABLE);
        }
    };
    #[cfg(unix)]
    let _nonblocking =
        command_line::serves_protocol(&arguments).then(standard_output_nonblocking).flatten();
    let mut standard_output = io::stdout().lock();
    let mut standard_error = io::stderr().lock();
    let exit =
        command_line::run(&arguments, &executable, &mut standard_output, &mut standard_error);
    ExitCode::from(u8::try_from(exit).unwrap_or(command_line::EXIT_RUNTIME_UNUSABLE))
}
