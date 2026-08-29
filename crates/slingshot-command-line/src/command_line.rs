//! The argument surface and dispatcher of the product executable.
//!
//! Standard output carries the structured result of the command and nothing
//! else; every diagnostic goes to the diagnostic stream. Each way a command can
//! end has its own exit status, so a caller can tell a daemon that never became
//! responsive from one that refused, and both from a target it cannot name.

use std::io::Write;
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use serde::Serialize;
use slingshot_local_protocol::foundation_contract::FoundationContract;

use crate::daemon_entry::{self, DaemonEntryArguments, DaemonEntryOutcome};
use crate::explicit_daemon_start::{self, StartFailure, TargetRuntime};

/// Exit status of a command that finished.
pub const EXIT_SUCCESS: u8 = 0;

/// Exit status of a command whose arguments do not name a target.
pub const EXIT_TARGET_UNUSABLE: u8 = 2;

/// Exit status of a command whose runtime state could not be used.
pub const EXIT_RUNTIME_UNUSABLE: u8 = 3;

/// Exit status of a start whose daemon could not be created.
pub const EXIT_DAEMON_UNSTARTABLE: u8 = 4;

/// Exit status of a start that did not converge inside its deadline.
pub const EXIT_DEADLINE_ELAPSED: u8 = 5;

/// Exit status of a probe the daemon refused.
pub const EXIT_DAEMON_REFUSED: u8 = 6;

/// Exit status of a start whose daemon reported an unusable readiness nonce.
pub const EXIT_READINESS_UNUSABLE: u8 = 7;

/// Exit status of a daemon process that found its namespace already owned.
pub const EXIT_ALREADY_OWNED: u8 = 8;

/// Request identifier a command uses when the caller supplies none.
const DEFAULT_REQUEST_IDENTIFIER: &str = "command-line";

/// The product command-line surface.
#[derive(Debug, Parser)]
#[command(name = "slingshot", version, about = "Slingshot command line")]
pub struct ProductArguments {
    /// Profile half of the target this invocation addresses.
    #[arg(long, global = true)]
    pub profile: Option<String>,
    /// Environment half of the target this invocation addresses.
    #[arg(long, global = true)]
    pub environment: Option<String>,
    /// Runtime root the target's objects live in, instead of this user's own.
    #[arg(long, global = true)]
    pub runtime_root: Option<PathBuf>,
    /// Command this invocation runs.
    #[command(subcommand)]
    pub command: ProductCommand,
}

/// Commands the product executable offers.
#[derive(Debug, Subcommand)]
pub enum ProductCommand {
    /// Work with the daemon that owns a target.
    Daemon {
        /// Action to take on that daemon.
        #[command(subcommand)]
        action: DaemonAction,
    },
}

/// Actions the daemon command offers.
#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum DaemonAction {
    /// Reach the daemon that owns the target, creating it if nobody has.
    Start,
    /// Report whether a daemon already owns the target.
    Ping,
    /// Serve the target. This is how a start creates its child.
    #[command(hide = true)]
    Serve,
}

/// Reason a command could not build the target it addresses.
#[derive(Debug, thiserror::Error)]
pub enum CommandFailure {
    /// The invocation named no profile or no environment.
    #[error("both --profile and --environment name the target this command addresses")]
    TargetIncomplete,
    /// The runtime root could not be resolved.
    #[error("the runtime root could not be resolved: {0}")]
    RuntimeRootUnavailable(String),
    /// A start or probe failed.
    #[error(transparent)]
    Start(#[from] StartFailure),
    /// A daemon process failed.
    #[error(transparent)]
    DaemonEntry(#[from] daemon_entry::DaemonEntryFailure),
    /// The result could not be written.
    #[error("the result could not be written: {0}")]
    OutputUnavailable(String),
}

impl CommandFailure {
    /// Returns the exit status this failure ends the process with.
    #[must_use]
    pub fn exit_status(&self) -> u8 {
        match self {
            Self::TargetIncomplete | Self::RuntimeRootUnavailable(_) => EXIT_TARGET_UNUSABLE,
            Self::Start(StartFailure::Target(_)) => EXIT_TARGET_UNUSABLE,
            Self::Start(StartFailure::Runtime(_)) | Self::DaemonEntry(_) => EXIT_RUNTIME_UNUSABLE,
            Self::Start(StartFailure::Unstartable(_)) => EXIT_DAEMON_UNSTARTABLE,
            Self::Start(StartFailure::DeadlineElapsed(_)) => EXIT_DEADLINE_ELAPSED,
            Self::Start(StartFailure::Refused(_)) => EXIT_DAEMON_REFUSED,
            Self::Start(StartFailure::InvalidReadinessNonce(_)) => EXIT_READINESS_UNUSABLE,
            Self::OutputUnavailable(_) => EXIT_RUNTIME_UNUSABLE,
        }
    }
}

/// Returns the runtime root a target's objects live in.
fn resolve_runtime_root(supplied: Option<PathBuf>) -> Result<PathBuf, CommandFailure> {
    if let Some(root) = supplied {
        return Ok(root);
    }
    let directories = directories::ProjectDirs::from("", "", "slingshot")
        .ok_or_else(|| CommandFailure::RuntimeRootUnavailable("no home directory".to_owned()))?;
    let root = directories.runtime_dir().unwrap_or_else(|| directories.data_dir());
    Ok(root.to_path_buf())
}

/// Builds the target one invocation addresses.
fn resolve_target(arguments: &ProductArguments) -> Result<TargetRuntime, CommandFailure> {
    let (Some(profile), Some(environment)) = (&arguments.profile, &arguments.environment) else {
        return Err(CommandFailure::TargetIncomplete);
    };
    Ok(TargetRuntime {
        runtime_root: resolve_runtime_root(arguments.runtime_root.clone())?,
        profile: profile.clone(),
        environment: environment.clone(),
    })
}

/// Writes one structured result to the caller's result stream.
fn write_result(output: &mut dyn Write, result: &impl Serialize) -> Result<(), CommandFailure> {
    let rendered = serde_json::to_string(result)
        .map_err(|failure| CommandFailure::OutputUnavailable(failure.to_string()))?;
    writeln!(output, "{rendered}")
        .and_then(|()| output.flush())
        .map_err(|failure| CommandFailure::OutputUnavailable(failure.to_string()))
}

/// Runs one already-parsed invocation.
///
/// # Errors
///
/// Returns [`CommandFailure`] when the invocation does not name a usable
/// target, the runtime state cannot be used, or the command itself fails.
pub async fn run(
    arguments: ProductArguments,
    executable: &std::path::Path,
    output: &mut dyn Write,
) -> Result<u8, CommandFailure> {
    let contract = FoundationContract::embedded();
    let target = resolve_target(&arguments)?;
    let ProductCommand::Daemon { action } = arguments.command;
    match action {
        DaemonAction::Start => {
            let report = explicit_daemon_start::explicit_start(
                &contract,
                &target,
                executable,
                DEFAULT_REQUEST_IDENTIFIER,
            )
            .await?;
            write_result(output, &report)?;
            Ok(EXIT_SUCCESS)
        }
        DaemonAction::Ping => {
            let report = explicit_daemon_start::existing_only_ping(
                &contract,
                &target,
                DEFAULT_REQUEST_IDENTIFIER,
            )
            .await?;
            write_result(output, &report)?;
            Ok(EXIT_SUCCESS)
        }
        DaemonAction::Serve => {
            let entry = DaemonEntryArguments::new(
                &target.runtime_root,
                &target.profile,
                &target.environment,
            );
            let shutdown = tokio_util::sync::CancellationToken::new();
            match daemon_entry::run_daemon_entry(&contract, &entry, shutdown).await? {
                DaemonEntryOutcome::Served => Ok(EXIT_SUCCESS),
                DaemonEntryOutcome::AlreadyOwned => Ok(EXIT_ALREADY_OWNED),
            }
        }
    }
}
