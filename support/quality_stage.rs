//! Monotonic command timing for the offline repository gate.
//!
//! Compiled with the repository's pinned rustc before measured stages begin.
//! This helper inherits the command's streams and environment and preserves its
//! exit code. A missing command or unwritable evidence file refuses the stage.

use std::io::Write;
use std::process::{Command, ExitCode};
use std::time::Instant;

/// Exit code for a command terminated without an ordinary status or a failed helper.
const FAILED: u8 = 1;
/// Stage, evidence path and executable precede optional command arguments.
const REQUIRED_ARGUMENTS: usize = 3;

/// Runs one stage and records its duration, including a failing command.
fn run() -> Result<ExitCode, Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if arguments.len() < REQUIRED_ARGUMENTS {
        return Err("usage: quality_stage STAGE TIMINGS COMMAND [ARGUMENTS...]".into());
    }
    let mut evidence = std::fs::OpenOptions::new().create(true).append(true).open(&arguments[1])?;
    let started = Instant::now();
    let outcome = Command::new(&arguments[2]).args(&arguments[REQUIRED_ARGUMENTS..]).status();
    let elapsed = started.elapsed().as_secs_f64();
    let status = outcome
        .as_ref()
        .ok()
        .and_then(std::process::ExitStatus::code)
        .and_then(|code| u8::try_from(code).ok())
        .unwrap_or(FAILED);
    let stage = arguments[0].to_string_lossy();
    writeln!(evidence, "{stage}\t{elapsed:.6}\t{status}")?;
    println!("timing: {stage}\t{elapsed:.6}\t{status}");
    outcome?;
    Ok(ExitCode::from(status))
}

/// Renders helper failures without converting a failed stage into success.
fn main() -> ExitCode {
    match run() {
        Ok(status) => status,
        Err(failure) => {
            eprintln!("quality timing: {failure}");
            ExitCode::from(FAILED)
        }
    }
}
