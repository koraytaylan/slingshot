//! Process entry point for the repository-command executable.
//!
//! The entry point separates the executable path from the arguments, resolves
//! the working directory, dispatches one repository command by name, and maps
//! the outcome onto a process exit code. Every command body lives in the
//! library, so a command stays testable without spawning a process.

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use slingshot_development::{
    RepositoryCommandFailure, dependency_direction, finite_state_machine_compatibility,
    release_input_cache, rustsec_advisory_pin, source_policy,
};

/// Number of leading process arguments that carry the executable path.
const EXECUTABLE_PATH_ARGUMENT_COUNT: usize = 1;

/// Name of the command that emits resolved Cargo workspace metadata.
const WORKSPACE_METADATA_COMMAND: &str = "workspace-metadata";

/// Name of the command that checks the workspace dependency direction.
const DEPENDENCY_DIRECTION_COMMAND: &str = "dependency-direction";

/// Name of the command that checks every repository source policy rule.
const SOURCE_POLICY_COMMAND: &str = "source-policy";

/// Name of the command that verifies the pinned advisory snapshot.
const RUSTSEC_PIN_COMMAND: &str = "rustsec-advisory-pin";

/// Name of the command that proposes pin bytes for a reviewed candidate.
const RUSTSEC_PIN_REVIEW_COMMAND: &str = "rustsec-pin-review";

/// Name of the command that writes the manifest for a cache just fetched.
const PREPARE_CACHE_COMMAND: &str = "prepare-locked-source-cache";

/// Name of the command that verifies the cache a release builds from.
const VERIFY_CACHE_COMMAND: &str = "verify-locked-source-cache";

/// Option naming the cache that was just prepared.
const PREPARE_CACHE_OPTION: &str = "--output-directory";

/// Option naming the cache being verified.
const VERIFY_CACHE_OPTION: &str = "--cache-set";

/// How many arguments one named option and its value occupy.
const OPTION_AND_VALUE: usize = 2;

/// Name of the command that bounds a supplied Cargo home.
const VERIFY_SEED_COMMAND: &str = "verify-cargo-home-seed";

/// Option naming the supplied Cargo home.
const VERIFY_SEED_OPTION: &str = "--cargo-home-seed";

/// Name of the internal command that runs a daemon with a scripted executor.
///
/// Internal because it exists for tests. It is a subcommand of this binary
/// rather than a binary of its own, so the workspace keeps exactly the two
/// targets a release accounts for.
const TEST_DAEMON_COMMAND: &str = slingshot_development::slingshot_test_daemon::TEST_DAEMON_COMMAND;

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
        RUSTSEC_PIN_COMMAND => verify_advisory_pin(working_directory, output),
        RUSTSEC_PIN_REVIEW_COMMAND => review_advisory_pin(arguments, output),
        VERIFY_SEED_COMMAND => verify_cargo_home_seed(arguments, working_directory, output),
        PREPARE_CACHE_COMMAND => prepare_locked_source_cache(arguments, working_directory, output),
        VERIFY_CACHE_COMMAND => verify_locked_source_cache(arguments, working_directory, output),
        TEST_DAEMON_COMMAND => run_test_daemon(output),
        _ => Err(RepositoryCommandFailure::UnknownCommand(requested.clone())),
    }
}

/// Runs a daemon composed with a scripted executor.
///
/// Reports the composition it built rather than serving, because what this
/// subcommand exists to prove from outside is that the development binary can
/// reach the fake and the product binary cannot.
fn run_test_daemon(output: &mut dyn Write) -> Result<(), RepositoryCommandFailure> {
    use slingshot_daemon::unavailable_operation_executor::UnavailableOperationExecutor;
    use slingshot_development::slingshot_test_daemon::TestDaemonComposition;

    let composition = TestDaemonComposition::new(UnavailableOperationExecutor::outcome());
    writeln!(output, "{TEST_DAEMON_COMMAND} composed a scripted executor")
        .map_err(|failure| RepositoryCommandFailure::OutputUnavailable(failure.to_string()))?;
    drop(composition);
    Ok(())
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

/// Reads the pin and the one named checkout, and verifies they are the same.
fn verify_advisory_pin(
    working_directory: &Path,
    output: &mut dyn Write,
) -> Result<(), RepositoryCommandFailure> {
    let workspace_root = slingshot_development::locate_workspace_root(working_directory)?;
    let refuse = |failure: rustsec_advisory_pin::PinFailure| RepositoryCommandFailure::ToolFailed {
        program: RUSTSEC_PIN_COMMAND.to_owned(),
        reason: failure.to_string(),
    };
    let path = workspace_root.join(rustsec_advisory_pin::PIN_PATH);
    let text = std::fs::read_to_string(&path).map_err(|failure| {
        RepositoryCommandFailure::PathUnreadable { path, reason: failure.to_string() }
    })?;
    let pin = rustsec_advisory_pin::parse_pin(&text).map_err(refuse)?;
    let checkout = rustsec_advisory_pin::named_checkout().map_err(refuse)?;
    let verified = rustsec_advisory_pin::verify(&pin, &checkout).map_err(refuse)?;
    let rendered = serde_json::to_string(&verified).unwrap_or_default();
    writeln!(output, "{rendered}")
        .map_err(|failure| RepositoryCommandFailure::OutputUnavailable(failure.to_string()))
}

/// Reads one deliberately named candidate and proposes its pin bytes.
fn review_advisory_pin(
    arguments: &[String],
    output: &mut dyn Write,
) -> Result<(), RepositoryCommandFailure> {
    let refuse = |reason: String| RepositoryCommandFailure::ToolFailed {
        program: RUSTSEC_PIN_REVIEW_COMMAND.to_owned(),
        reason,
    };
    let candidate = arguments
        .get(1)
        .ok_or_else(|| refuse("name the candidate checkout to propose a pin from".to_owned()))?;
    let snapshot = rustsec_advisory_pin::read_snapshot(Path::new(candidate))
        .map_err(|failure| refuse(failure.to_string()))?;
    write!(output, "{}", rustsec_advisory_pin::propose_pin(&snapshot))
        .map_err(|failure| RepositoryCommandFailure::OutputUnavailable(failure.to_string()))
}

/// Returns the directory one named option carries.
fn named_directory(
    arguments: &[String],
    option: &str,
    program: &str,
) -> Result<PathBuf, RepositoryCommandFailure> {
    let refuse = |reason: String| RepositoryCommandFailure::ToolFailed {
        program: program.to_owned(),
        reason,
    };
    let named = arguments
        .windows(OPTION_AND_VALUE)
        .find(|pair| pair[0] == option)
        .map(|pair| pair[1].clone())
        .ok_or_else(|| refuse(format!("name the cache with {option}")))?;
    let path = PathBuf::from(&named);
    if path.is_absolute() {
        Ok(path)
    } else {
        Err(refuse(format!("{named} is not an absolute path")))
    }
}

/// Reads the one authority for what a supplied Cargo home may be.
fn cargo_home_limits(
    working_directory: &Path,
    program: &str,
) -> Result<finite_state_machine_compatibility::SeedLimits, RepositoryCommandFailure> {
    let workspace_root = slingshot_development::locate_workspace_root(working_directory)?;
    let path = workspace_root.join(finite_state_machine_compatibility::MANIFEST_PATH);
    let text = std::fs::read_to_string(&path).map_err(|failure| {
        RepositoryCommandFailure::PathUnreadable { path, reason: failure.to_string() }
    })?;
    let pin = finite_state_machine_compatibility::FiniteStateMachineCompatibilityPin::parse(&text)
        .map_err(|failure| RepositoryCommandFailure::ToolFailed {
            program: program.to_owned(),
            reason: failure.to_string(),
        })?;
    Ok(pin.cargo_home_seed)
}

/// Bounds one supplied Cargo home against the committed compatibility manifest.
///
/// The manifest is the sole authority for every limit, so this reads it rather
/// than restating any of its values.
fn verify_cargo_home_seed(
    arguments: &[String],
    working_directory: &Path,
    output: &mut dyn Write,
) -> Result<(), RepositoryCommandFailure> {
    let seed = named_directory(arguments, VERIFY_SEED_OPTION, VERIFY_SEED_COMMAND)?;
    let limits = cargo_home_limits(working_directory, VERIFY_SEED_COMMAND)?;
    let survey =
        finite_state_machine_compatibility::verify_seed(&seed, &limits).map_err(|failure| {
            RepositoryCommandFailure::ToolFailed {
                program: VERIFY_SEED_COMMAND.to_owned(),
                reason: failure.to_string(),
            }
        })?;
    writeln!(
        output,
        "this Cargo home holds {} in {}, {} altogether",
        counted(survey.files, "file", "files"),
        counted(survey.directories, "directory", "directories"),
        counted(survey.aggregate_file_bytes, "byte", "bytes")
    )
    .map_err(|failure| RepositoryCommandFailure::OutputUnavailable(failure.to_string()))
}

/// Returns one count worded with the noun that suits it.
fn counted(count: u64, singular: &str, plural: &str) -> String {
    if count == 1 { format!("{count} {singular}") } else { format!("{count} {plural}") }
}

/// Reads what this repository declares a release may build from.
fn release_input_declaration(
    working_directory: &Path,
    program: &str,
) -> Result<(release_input_cache::CacheDeclaration, String), RepositoryCommandFailure> {
    let workspace_root = slingshot_development::locate_workspace_root(working_directory)?;
    let path = workspace_root.join(release_input_cache::DECLARATION_PATH);
    let text = std::fs::read_to_string(&path).map_err(|failure| {
        RepositoryCommandFailure::PathUnreadable { path, reason: failure.to_string() }
    })?;
    let refuse =
        |failure: release_input_cache::CacheRefusal| RepositoryCommandFailure::ToolFailed {
            program: program.to_owned(),
            reason: failure.to_string(),
        };
    let declaration = release_input_cache::parse_declaration(&text).map_err(refuse)?;
    let digest = release_input_cache::lockfile_digest(&workspace_root).map_err(refuse)?;
    Ok((declaration, digest))
}

/// Writes the manifest describing the cache that was just fetched.
fn prepare_locked_source_cache(
    arguments: &[String],
    working_directory: &Path,
    output: &mut dyn Write,
) -> Result<(), RepositoryCommandFailure> {
    let cache = named_directory(arguments, PREPARE_CACHE_OPTION, PREPARE_CACHE_COMMAND)?;
    let (declaration, lock) = release_input_declaration(working_directory, PREPARE_CACHE_COMMAND)?;
    let limits = cargo_home_limits(working_directory, PREPARE_CACHE_COMMAND)?;
    let manifest =
        release_input_cache::prepare(&cache, &declaration, &limits, &lock).map_err(|failure| {
            RepositoryCommandFailure::ToolFailed {
                program: PREPARE_CACHE_COMMAND.to_owned(),
                reason: failure.to_string(),
            }
        })?;
    let held = counted(manifest.entries, "entry", "entries");
    writeln!(output, "this cache holds {held} a release may build from")
        .map_err(|failure| RepositoryCommandFailure::OutputUnavailable(failure.to_string()))
}

/// Verifies the cache a release is about to build from.
fn verify_locked_source_cache(
    arguments: &[String],
    working_directory: &Path,
    output: &mut dyn Write,
) -> Result<(), RepositoryCommandFailure> {
    let cache = named_directory(arguments, VERIFY_CACHE_OPTION, VERIFY_CACHE_COMMAND)?;
    let (declaration, lock) = release_input_declaration(working_directory, VERIFY_CACHE_COMMAND)?;
    let limits = cargo_home_limits(working_directory, VERIFY_CACHE_COMMAND)?;
    let manifest =
        release_input_cache::verified(&cache, &declaration, &limits, &lock).map_err(|failure| {
            RepositoryCommandFailure::ToolFailed {
                program: VERIFY_CACHE_COMMAND.to_owned(),
                reason: failure.to_string(),
            }
        })?;
    let held = counted(manifest.entries, "entry", "entries");
    writeln!(output, "this cache is the one prepared for this lockfile and holds {held}")
        .map_err(|failure| RepositoryCommandFailure::OutputUnavailable(failure.to_string()))
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
