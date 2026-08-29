//! Command-line adapter for the `slingshot` product executable.
//!
//! The workspace dependency contract lets this crate depend on
//! `slingshot-local-protocol`, `slingshot-configuration`, and
//! `slingshot-daemon`. The process entry point stays thin and delegates here,
//! so command behavior stays testable without spawning a process. This commit
//! implements the product version proof and declares the crate's module
//! families as documentation-only roots.

pub mod command_line;
pub mod commands;
pub mod daemon_connection;
pub mod daemon_entry;
pub mod explicit_daemon_start;
pub mod model_context_protocol;
pub mod platform_runtime;

use std::fmt;
use std::io::Write;

/// Product name printed ahead of the version on the version line.
pub const PRODUCT_NAME: &str = "slingshot";

/// Argument that requests the single product version line.
pub const VERSION_ARGUMENT: &str = "--version";

/// Reason the command-line surface refused an invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandLineFailure {
    /// The invocation carried no argument after the executable path.
    MissingArgument,
    /// The invocation named an argument this commit does not implement.
    UnknownArgument(String),
    /// The version line could not be written to the supplied output.
    OutputUnavailable(String),
}

impl fmt::Display for CommandLineFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingArgument => {
                write!(formatter, "expected {VERSION_ARGUMENT}, but no argument was supplied")
            }
            Self::UnknownArgument(supplied) => {
                write!(formatter, "unsupported argument {supplied:?}; expected {VERSION_ARGUMENT}")
            }
            Self::OutputUnavailable(reason) => {
                write!(formatter, "the version line could not be written: {reason}")
            }
        }
    }
}

impl std::error::Error for CommandLineFailure {}

/// Returns the single line printed for a version request.
///
/// The version comes from the package manifest, so the printed value and the
/// workspace version cannot drift apart.
#[must_use]
pub fn version_line() -> String {
    format!("{PRODUCT_NAME} {}", env!("CARGO_PKG_VERSION"))
}

/// Runs the command-line surface over already-separated arguments.
///
/// `arguments` excludes the executable path. The version line is written and
/// flushed through `output`; nothing else is read, written, or created, so an
/// invocation leaves no runtime state behind.
///
/// # Errors
///
/// Returns [`CommandLineFailure::MissingArgument`] when `arguments` is empty,
/// [`CommandLineFailure::UnknownArgument`] when the first argument is not
/// [`VERSION_ARGUMENT`], and [`CommandLineFailure::OutputUnavailable`] when
/// `output` rejects the line or its flush.
pub fn execute(arguments: &[String], output: &mut dyn Write) -> Result<(), CommandLineFailure> {
    let requested = arguments.first().ok_or(CommandLineFailure::MissingArgument)?;
    if requested != VERSION_ARGUMENT {
        return Err(CommandLineFailure::UnknownArgument(requested.clone()));
    }
    writeln!(output, "{}", version_line())
        .and_then(|()| output.flush())
        .map_err(|failure| CommandLineFailure::OutputUnavailable(failure.to_string()))
}
