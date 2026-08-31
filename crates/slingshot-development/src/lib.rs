//! Repository policy, orchestration, compatibility, and release commands.
//!
//! This crate is the outermost tooling layer of the workspace. It may depend
//! inward on the product crates and on `slingshot-test-support`, and no other
//! crate may depend on it. This commit implements the workspace-metadata
//! command body, declares the crate's module families as documentation-only
//! roots, and declares the profile-authentication harness leaf as
//! documentation-only structure.

pub mod coverage_fuzzing_tool;
pub mod daemon_chaos_subject;
pub mod dependency_direction;
pub mod finite_state_machine_acknowledgement;
pub mod finite_state_machine_compatibility;
pub mod finite_state_machine_handler_validation;
pub mod finite_state_machine_process_harness;
pub mod platform_runtime_contract;
pub mod profile_authentication_harness;
pub mod release_input_cache;
pub mod rustsec_advisory_pin;
pub mod slingshot_test_daemon;
pub mod script_policy;
pub mod source_policy;
pub mod workflow_policy;
pub mod supported_platform_matrix;
pub mod test_daemon_faults;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

/// File name of a Cargo manifest.
pub const CARGO_MANIFEST_FILE_NAME: &str = "Cargo.toml";

/// Table header that marks a Cargo manifest as the workspace root manifest.
pub const WORKSPACE_TABLE_HEADER: &str = "[workspace]";

/// Environment variable Cargo sets to the Cargo executable running a build.
const CARGO_EXECUTABLE_VARIABLE: &str = "CARGO";

/// Fallback Cargo executable name when the environment names none.
const CARGO_EXECUTABLE_FALLBACK: &str = "cargo";

/// Cargo subcommand that reports resolved workspace metadata.
const METADATA_SUBCOMMAND: &str = "metadata";

/// Cargo flag that refuses to change the committed lockfile.
const LOCKED_FLAG: &str = "--locked";

/// Cargo flag that selects the stable metadata schema.
const FORMAT_VERSION_FLAG: &str = "--format-version";

/// Stable Cargo metadata schema selected by this crate.
const FORMAT_VERSION_VALUE: &str = "1";

/// Cargo flag that restricts metadata to workspace members.
const NO_DEPENDENCIES_FLAG: &str = "--no-deps";

/// Cargo flag that names the manifest a subcommand should read.
const MANIFEST_PATH_FLAG: &str = "--manifest-path";

/// Reason a repository command refused to run or could not finish.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepositoryCommandFailure {
    /// The invocation carried no repository command name.
    MissingCommand,
    /// The invocation named a command the dispatcher does not implement.
    UnknownCommand(String),
    /// No ancestor of the starting directory held a workspace root manifest.
    WorkspaceRootNotFound(PathBuf),
    /// A repository path could not be read.
    PathUnreadable {
        /// Path the command tried to read.
        path: PathBuf,
        /// Operating-system reason the read failed.
        reason: String,
    },
    /// A child tool could not be started or reported a failing status.
    ToolFailed {
        /// Program the command tried to run.
        program: String,
        /// Reason the run failed, including any captured diagnostics.
        reason: String,
    },
    /// The command result could not be written to the supplied output.
    OutputUnavailable(String),
}

impl ::core::fmt::Display for RepositoryCommandFailure {
    fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        match self {
            Self::MissingCommand => {
                write!(formatter, "expected a repository command name, but none was supplied")
            }
            Self::UnknownCommand(supplied) => {
                write!(formatter, "unknown repository command {supplied:?}")
            }
            Self::WorkspaceRootNotFound(start) => {
                write!(formatter, "no workspace root manifest above {}", start.display())
            }
            Self::PathUnreadable { path, reason } => {
                write!(formatter, "{} could not be read: {reason}", path.display())
            }
            Self::ToolFailed { program, reason } => {
                write!(formatter, "{program} failed: {reason}")
            }
            Self::OutputUnavailable(reason) => {
                write!(formatter, "the command result could not be written: {reason}")
            }
        }
    }
}

impl std::error::Error for RepositoryCommandFailure {}

/// Returns the Cargo executable that should run repository subcommands.
///
/// Cargo exports its own path while it runs a build or a test, so a nested
/// invocation uses the same toolchain instead of whichever `cargo` appears
/// first on the search path.
#[must_use]
pub fn cargo_executable() -> PathBuf {
    std::env::var_os(CARGO_EXECUTABLE_VARIABLE)
        .map_or_else(|| PathBuf::from(CARGO_EXECUTABLE_FALLBACK), PathBuf::from)
}

/// Walks upward from `start` and returns the directory holding the workspace
/// root manifest.
///
/// A manifest is the workspace root when its text contains the
/// [`WORKSPACE_TABLE_HEADER`] table header.
///
/// # Errors
///
/// Returns [`RepositoryCommandFailure::PathUnreadable`] when a candidate
/// manifest exists but cannot be read, and
/// [`RepositoryCommandFailure::WorkspaceRootNotFound`] when no ancestor holds
/// a workspace root manifest.
pub fn locate_workspace_root(start: &Path) -> Result<PathBuf, RepositoryCommandFailure> {
    for directory in start.ancestors() {
        let manifest = directory.join(CARGO_MANIFEST_FILE_NAME);
        if !manifest.is_file() {
            continue;
        }
        let text = std::fs::read_to_string(&manifest).map_err(|failure| {
            RepositoryCommandFailure::PathUnreadable {
                path: manifest.clone(),
                reason: failure.to_string(),
            }
        })?;
        if text.lines().any(|line| line.trim_end() == WORKSPACE_TABLE_HEADER) {
            return Ok(directory.to_path_buf());
        }
    }
    Err(RepositoryCommandFailure::WorkspaceRootNotFound(start.to_path_buf()))
}

/// Writes resolved Cargo metadata for the workspace rooted at `workspace_root`.
///
/// The metadata is produced by `cargo metadata --locked --format-version 1
/// --no-deps`, so the command never edits the committed lockfile and never
/// resolves registry dependencies.
///
/// # Errors
///
/// Returns [`RepositoryCommandFailure::ToolFailed`] when Cargo cannot be
/// started or exits with a failing status, and
/// [`RepositoryCommandFailure::OutputUnavailable`] when `output` rejects the
/// metadata bytes.
pub fn emit_workspace_metadata(
    workspace_root: &Path,
    output: &mut dyn Write,
) -> Result<(), RepositoryCommandFailure> {
    let program = cargo_executable();
    let produced = Command::new(&program)
        .arg(METADATA_SUBCOMMAND)
        .arg(LOCKED_FLAG)
        .arg(FORMAT_VERSION_FLAG)
        .arg(FORMAT_VERSION_VALUE)
        .arg(NO_DEPENDENCIES_FLAG)
        .arg(MANIFEST_PATH_FLAG)
        .arg(workspace_root.join(CARGO_MANIFEST_FILE_NAME))
        .output()
        .map_err(|failure| RepositoryCommandFailure::ToolFailed {
            program: program.display().to_string(),
            reason: failure.to_string(),
        })?;
    if !produced.status.success() {
        return Err(RepositoryCommandFailure::ToolFailed {
            program: program.display().to_string(),
            reason: String::from_utf8_lossy(&produced.stderr).trim().to_owned(),
        });
    }
    output
        .write_all(&produced.stdout)
        .and_then(|()| output.flush())
        .map_err(|failure| RepositoryCommandFailure::OutputUnavailable(failure.to_string()))
}
