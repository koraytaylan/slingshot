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
    RepositoryCommandFailure, coverage_fuzzing_tool, dependency_direction,
    finite_state_machine_compatibility, github_automation_authority, release_acceptance,
    release_artifacts, release_input_cache, rustsec_advisory_pin, source_policy,
    supported_platform_matrix,
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

/// Name of the command that verifies the pinned coverage-fuzzing bundle.
const VERIFY_FUZZING_TOOL_COMMAND: &str = "verify-coverage-fuzzing-tool";

/// Name of the command that acquires and builds the pinned coverage tool.
const PREPARE_FUZZING_TOOL_COMMAND: &str = "prepare-coverage-fuzzing-tool";

/// Name of the command that validates the acceptance isolation contract.
const CONTAINER_COMMAND: &str = "release-acceptance-container";

/// Name of the command that verifies one acceptance manifest.
const VERIFY_ACCEPTANCE_COMMAND: &str = "verify-release-acceptance";

/// Name of the command that builds one row's release archive.
const PACKAGE_COMMAND: &str = "package-release-artifacts";

/// Name of the command that verifies one row's release archive.
const VERIFY_ARTIFACTS_COMMAND: &str = "verify-release-artifacts";

/// Name of the command that validates the hosted automation authority.
const AUTHORITY_COMMAND: &str = "github-automation-authority";

/// Name of the command that proposes a reviewed repository identity.
const REPOSITORY_REVIEW_COMMAND: &str = "github-repository-review";

/// Variables one hosted run reports itself through.
const REPORTED_VARIABLES: [&str; 5] = [
    "SLINGSHOT_REPORTED_REPOSITORY",
    "SLINGSHOT_REPORTED_REPOSITORY_IDENTIFIER",
    "SLINGSHOT_REPORTED_OWNER_IDENTIFIER",
    "SLINGSHOT_REPORTED_WORKFLOW",
    "SLINGSHOT_REPORTED_RUNNER",
];

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
///
/// Two tables rather than one, split where the subject changes: the commands
/// that inspect this repository, and the commands that adapt it to a provider
/// or prepare a release. A reader looking for one knows which half to read.
fn dispatch(
    arguments: &[String],
    working_directory: &Path,
    output: &mut dyn Write,
) -> Result<(), RepositoryCommandFailure> {
    let requested = arguments.first().ok_or(RepositoryCommandFailure::MissingCommand)?;
    match inspection_command(requested, arguments, working_directory, output) {
        Some(outcome) => outcome,
        None => match provider_command(requested, arguments, working_directory, output) {
            Some(outcome) => outcome,
            None => release_command(requested, arguments, working_directory, output),
        },
    }
}

/// Runs one command that adapts this repository to a provider, when it is one.
fn provider_command(
    requested: &str,
    arguments: &[String],
    working_directory: &Path,
    output: &mut dyn Write,
) -> Option<Result<(), RepositoryCommandFailure>> {
    let outcome = match requested {
        AUTHORITY_COMMAND => check_automation_authority(working_directory, output),
        REPOSITORY_REVIEW_COMMAND => {
            review_repository_identity(arguments, working_directory, output)
        }
        CONTAINER_COMMAND => check_acceptance_container(working_directory, output),
        VERIFY_FUZZING_TOOL_COMMAND => {
            verify_coverage_fuzzing_tool(arguments, working_directory, output)
        }
        PREPARE_FUZZING_TOOL_COMMAND => {
            prepare_coverage_fuzzing_tool(arguments, working_directory, output)
        }
        _ => return None,
    };
    Some(outcome)
}

/// Runs one command that inspects this repository, when the name is one.
fn inspection_command(
    requested: &str,
    arguments: &[String],
    working_directory: &Path,
    output: &mut dyn Write,
) -> Option<Result<(), RepositoryCommandFailure>> {
    let outcome = match requested {
        WORKSPACE_METADATA_COMMAND => emit_metadata(working_directory, output),
        DEPENDENCY_DIRECTION_COMMAND => check_dependency_direction(working_directory, output),
        SOURCE_POLICY_COMMAND => check_source_policy(working_directory, output),
        RUSTSEC_PIN_COMMAND => verify_advisory_pin(working_directory, output),
        RUSTSEC_PIN_REVIEW_COMMAND => review_advisory_pin(arguments, output),
        VERIFY_SEED_COMMAND => verify_cargo_home_seed(arguments, working_directory, output),
        TEST_DAEMON_COMMAND => run_test_daemon(output),
        _ => return None,
    };
    Some(outcome)
}

/// Runs one command that adapts this repository to a provider or a release.
fn release_command(
    requested: &str,
    arguments: &[String],
    working_directory: &Path,
    output: &mut dyn Write,
) -> Result<(), RepositoryCommandFailure> {
    match requested {
        PREPARE_CACHE_COMMAND => prepare_locked_source_cache(arguments, working_directory, output),
        VERIFY_CACHE_COMMAND => verify_locked_source_cache(arguments, working_directory, output),
        PACKAGE_COMMAND => package_release_artifacts(arguments, working_directory, output),
        VERIFY_ARTIFACTS_COMMAND => verify_release_artifacts(arguments, working_directory, output),
        VERIFY_ACCEPTANCE_COMMAND => verify_release_acceptance(arguments, output),
        _ => Err(RepositoryCommandFailure::UnknownCommand(requested.to_owned())),
    }
}

/// Emits the resolved workspace metadata.
fn emit_metadata(
    working_directory: &Path,
    output: &mut dyn Write,
) -> Result<(), RepositoryCommandFailure> {
    let workspace_root = slingshot_development::locate_workspace_root(working_directory)?;
    slingshot_development::emit_workspace_metadata(&workspace_root, output)
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

/// Reads the committed hosted-automation authority.
fn committed_authority(
    working_directory: &Path,
    program: &str,
) -> Result<github_automation_authority::GithubAutomationAuthority, RepositoryCommandFailure> {
    let workspace_root = slingshot_development::locate_workspace_root(working_directory)?;
    let path = workspace_root.join(github_automation_authority::AUTHORITY_PATH);
    let text = std::fs::read_to_string(&path).map_err(|failure| {
        RepositoryCommandFailure::PathUnreadable { path, reason: failure.to_string() }
    })?;
    github_automation_authority::parse_authority(&text).map_err(|failure| {
        RepositoryCommandFailure::ToolFailed {
            program: program.to_owned(),
            reason: failure.to_string(),
        }
    })
}

/// Requires this hosted run to be one the committed authority authorizes.
///
/// The run reports itself through the provider's own values, passed in
/// explicitly. Nothing about the machine is consulted: an ambient Git remote is
/// a file anybody can edit, and reading one would make the authority whatever
/// the checkout happened to contain.
fn check_automation_authority(
    working_directory: &Path,
    output: &mut dyn Write,
) -> Result<(), RepositoryCommandFailure> {
    let authority = committed_authority(working_directory, AUTHORITY_COMMAND)?;
    let refuse = |reason: String| RepositoryCommandFailure::ToolFailed {
        program: AUTHORITY_COMMAND.to_owned(),
        reason,
    };
    let mut reported = Vec::new();
    for named in REPORTED_VARIABLES {
        let held = std::env::var(named)
            .map_err(|_| refuse(format!("{named} names what this run reports, and is unset")))?;
        reported.push(held);
    }
    let run = github_automation_authority::ReportedRun {
        repository: reported[0].clone(),
        repository_identifier: reported[1].clone(),
        repository_owner_identifier: reported[2].clone(),
        workflow_path: reported[3].clone(),
        runner_selector: reported[4].clone(),
    };
    github_automation_authority::require_authorized(&authority, &run)
        .map_err(|failure| refuse(failure.to_string()))?;
    writeln!(output, "this run is the repository the owner confirmed, on a runner it maps")
        .map_err(|failure| RepositoryCommandFailure::OutputUnavailable(failure.to_string()))
}

/// Reads one named provider response and proposes the identity bytes to commit.
fn review_repository_identity(
    arguments: &[String],
    working_directory: &Path,
    output: &mut dyn Write,
) -> Result<(), RepositoryCommandFailure> {
    let refuse = |reason: String| RepositoryCommandFailure::ToolFailed {
        program: REPOSITORY_REVIEW_COMMAND.to_owned(),
        reason,
    };
    let named = arguments.get(1).ok_or_else(|| {
        refuse("name the provider response to propose an identity from".to_owned())
    })?;
    let authority = committed_authority(working_directory, REPOSITORY_REVIEW_COMMAND)?;
    let response = std::fs::read_to_string(named).map_err(|failure| {
        RepositoryCommandFailure::PathUnreadable {
            path: PathBuf::from(named),
            reason: failure.to_string(),
        }
    })?;
    let proposed =
        github_automation_authority::propose_repository_identifier(&authority, &response)
            .map_err(|failure| refuse(failure.to_string()))?;
    write!(output, "{proposed}")
        .map_err(|failure| RepositoryCommandFailure::OutputUnavailable(failure.to_string()))
}

/// Returns the value one named option carries.
fn named_value(
    arguments: &[String],
    option: &str,
    program: &str,
) -> Result<String, RepositoryCommandFailure> {
    arguments
        .windows(OPTION_AND_VALUE)
        .find(|pair| pair[0] == option)
        .map(|pair| pair[1].clone())
        .ok_or_else(|| RepositoryCommandFailure::ToolFailed {
            program: program.to_owned(),
            reason: format!("name it with {option}"),
        })
}

/// Returns the archive members and profile the supported matrix declares.
fn declared_archive(
    working_directory: &Path,
    triple: &str,
    program: &str,
) -> Result<(String, Vec<String>), RepositoryCommandFailure> {
    let workspace_root = slingshot_development::locate_workspace_root(working_directory)?;
    let path = workspace_root.join("support/platforms.toml");
    let text = std::fs::read_to_string(&path).map_err(|failure| {
        RepositoryCommandFailure::PathUnreadable { path, reason: failure.to_string() }
    })?;
    let refuse = |reason: String| RepositoryCommandFailure::ToolFailed {
        program: program.to_owned(),
        reason,
    };
    let matrix = supported_platform_matrix::parse_matrix(&text)
        .map_err(|failure| refuse(failure.to_string()))?;
    let row = matrix
        .target
        .into_iter()
        .find(|row| row.triple == triple)
        .ok_or_else(|| refuse(format!("{triple} is not a supported target")))?;
    Ok((row.archive_profile, row.archive_members))
}

/// Builds one row's release archive, its checksum manifest, and its evidence.
fn package_release_artifacts(
    arguments: &[String],
    working_directory: &Path,
    output: &mut dyn Write,
) -> Result<(), RepositoryCommandFailure> {
    let named = |option| named_value(arguments, option, PACKAGE_COMMAND);
    let triple = named("--row")?;
    let executable = named("--executable")?;
    let destination = PathBuf::from(named("--output-directory")?);
    let refuse = |reason: String| RepositoryCommandFailure::ToolFailed {
        program: PACKAGE_COMMAND.to_owned(),
        reason,
    };
    let (profile, members) = declared_archive(working_directory, &triple, PACKAGE_COMMAND)?;
    let workspace_root = slingshot_development::locate_workspace_root(working_directory)?;
    let built = collect_members(&members, Path::new(&executable), &workspace_root)?;
    let archive = destination.join(format!("slingshot-{triple}.{profile}"));
    release_artifacts::write_archive(&archive, &profile, &built, executable_member(&members))
        .map_err(|failure| refuse(failure.to_string()))?;
    let evidence = release_artifacts::EvidenceManifest {
        archive: archive.file_name().unwrap_or_default().to_string_lossy().to_string(),
        archive_sha256: digest_of_file(&archive)?,
        cache_sha256: digest_of_file(
            Path::new(&named("--cache-set")?).join("cache.json").as_path(),
        )?,
        format: release_artifacts::EVIDENCE_FORMAT.to_owned(),
        provider_run: std::env::var("SLINGSHOT_REPORTED_WORKFLOW").unwrap_or_default(),
        rustsec_review_record_sha256: digest_of_file(Path::new(&named(
            "--rustsec-owner-review-record",
        )?))?,
        source_commit: named("--source-commit")?,
        source_tree: named("--source-tree")?,
        toolchain: read_pinned_toolchain(&workspace_root)?,
        triple,
    };
    let rendered =
        toml::to_string_pretty(&evidence).map_err(|failure| refuse(failure.to_string()))?;
    std::fs::write(destination.join("evidence.toml"), rendered)
        .map_err(|failure| RepositoryCommandFailure::OutputUnavailable(failure.to_string()))?;
    writeln!(output, "{} holds {} members", archive.display(), built.len())
        .map_err(|failure| RepositoryCommandFailure::OutputUnavailable(failure.to_string()))
}

/// Returns the member that is the executable.
fn executable_member(members: &[String]) -> &str {
    members.iter().find(|held| held.starts_with("slingshot")).map_or("slingshot", String::as_str)
}

/// Reads every declared member's bytes.
fn collect_members(
    members: &[String],
    executable: &Path,
    workspace_root: &Path,
) -> Result<std::collections::BTreeMap<String, Vec<u8>>, RepositoryCommandFailure> {
    use sha2::Digest as _;

    let mut held = std::collections::BTreeMap::new();
    for name in members {
        if name == release_artifacts::CHECKSUM_MANIFEST {
            continue;
        }
        let source = if name == executable_member(members) {
            executable.to_path_buf()
        } else {
            workspace_root.join(name)
        };
        let bytes = std::fs::read(&source).map_err(|failure| {
            RepositoryCommandFailure::PathUnreadable { path: source, reason: failure.to_string() }
        })?;
        held.insert(name.clone(), bytes);
    }
    let digests = held
        .iter()
        .map(|(name, bytes)| (name.clone(), hex::encode(sha2::Sha256::digest(bytes))))
        .collect();
    let manifest = release_artifacts::render_checksum_manifest(&digests);
    held.insert(release_artifacts::CHECKSUM_MANIFEST.to_owned(), manifest.into_bytes());
    Ok(held)
}

/// Returns what one file's bytes digest to.
fn digest_of_file(path: &Path) -> Result<String, RepositoryCommandFailure> {
    use sha2::Digest as _;

    let bytes =
        std::fs::read(path).map_err(|failure| RepositoryCommandFailure::PathUnreadable {
            path: path.to_path_buf(),
            reason: failure.to_string(),
        })?;
    Ok(hex::encode(sha2::Sha256::digest(&bytes)))
}

/// Returns the toolchain this repository pins.
fn read_pinned_toolchain(workspace_root: &Path) -> Result<String, RepositoryCommandFailure> {
    let path = workspace_root.join("rust-toolchain.toml");
    let text = std::fs::read_to_string(&path).map_err(|failure| {
        RepositoryCommandFailure::PathUnreadable { path, reason: failure.to_string() }
    })?;
    let held: toml::Value =
        toml::from_str(&text).map_err(|failure| RepositoryCommandFailure::ToolFailed {
            program: PACKAGE_COMMAND.to_owned(),
            reason: failure.to_string(),
        })?;
    Ok(held["toolchain"]["channel"].as_str().unwrap_or_default().to_owned())
}

/// Verifies one row's release archive against what it claims.
fn verify_release_artifacts(
    arguments: &[String],
    working_directory: &Path,
    output: &mut dyn Write,
) -> Result<(), RepositoryCommandFailure> {
    let named = |option| named_value(arguments, option, VERIFY_ARTIFACTS_COMMAND);
    let archive = PathBuf::from(named("--archive")?);
    let refuse = |reason: String| RepositoryCommandFailure::ToolFailed {
        program: VERIFY_ARTIFACTS_COMMAND.to_owned(),
        reason,
    };
    let evidence_text = std::fs::read_to_string(named("--evidence")?)
        .map_err(|failure| refuse(failure.to_string()))?;
    let evidence = release_artifacts::parse_evidence(&evidence_text)
        .map_err(|failure| refuse(failure.to_string()))?;
    release_artifacts::require_evidence_binds(
        &evidence,
        &evidence.triple,
        &named("--source-commit")?,
        &named("--cache-sha256")?,
    )
    .map_err(|failure| refuse(failure.to_string()))?;
    let (profile, members) =
        declared_archive(working_directory, &evidence.triple, VERIFY_ARTIFACTS_COMMAND)?;
    let surveyed = release_artifacts::survey_archive(&archive, &profile)
        .map_err(|failure| refuse(failure.to_string()))?;
    let compressed = std::fs::metadata(&archive).map(|held| held.len()).unwrap_or_default();
    release_artifacts::require_admissible(&surveyed, &members, compressed)
        .map_err(|failure| refuse(failure.to_string()))?;
    writeln!(output, "this archive holds exactly the {} members its row declares", members.len())
        .map_err(|failure| RepositoryCommandFailure::OutputUnavailable(failure.to_string()))
}

/// Validates the committed isolation contract an acceptance run happens inside.
fn check_acceptance_container(
    working_directory: &Path,
    output: &mut dyn Write,
) -> Result<(), RepositoryCommandFailure> {
    let workspace_root = slingshot_development::locate_workspace_root(working_directory)?;
    let path = workspace_root.join(release_acceptance::CONTAINER_PATH);
    let text = std::fs::read_to_string(&path).map_err(|failure| {
        RepositoryCommandFailure::PathUnreadable { path, reason: failure.to_string() }
    })?;
    let held = release_acceptance::parse_container(&text).map_err(|failure| {
        RepositoryCommandFailure::ToolFailed {
            program: CONTAINER_COMMAND.to_owned(),
            reason: failure.to_string(),
        }
    })?;
    writeln!(
        output,
        "acceptance runs on {} with the network {} and every capability dropped",
        held.runtime.name, held.isolation.network
    )
    .map_err(|failure| RepositoryCommandFailure::OutputUnavailable(failure.to_string()))
}

/// Verifies one acceptance manifest records every gate, in order, all holding.
fn verify_release_acceptance(
    arguments: &[String],
    output: &mut dyn Write,
) -> Result<(), RepositoryCommandFailure> {
    let named = |option| named_value(arguments, option, VERIFY_ACCEPTANCE_COMMAND);
    let path = PathBuf::from(named("--manifest")?);
    let text = std::fs::read_to_string(&path).map_err(|failure| {
        RepositoryCommandFailure::PathUnreadable { path, reason: failure.to_string() }
    })?;
    let refuse = |reason: String| RepositoryCommandFailure::ToolFailed {
        program: VERIFY_ACCEPTANCE_COMMAND.to_owned(),
        reason,
    };
    let manifest =
        release_acceptance::parse_manifest(&text).map_err(|failure| refuse(failure.to_string()))?;
    release_acceptance::require_revision(&manifest, &named("--source-commit")?)
        .map_err(|failure| refuse(failure.to_string()))?;
    release_acceptance::require_complete(&manifest)
        .map_err(|failure| refuse(failure.to_string()))?;
    writeln!(output, "every one of the {} gates held", manifest.gates.len())
        .map_err(|failure| RepositoryCommandFailure::OutputUnavailable(failure.to_string()))
}

/// Reads the one authority for which coverage-fuzzing tool this repository uses.
fn coverage_fuzzing_pin(
    working_directory: &Path,
    program: &str,
) -> Result<coverage_fuzzing_tool::CoverageFuzzingPin, RepositoryCommandFailure> {
    let workspace_root = slingshot_development::locate_workspace_root(working_directory)?;
    let path = workspace_root.join(coverage_fuzzing_tool::PIN_PATH);
    let text = std::fs::read_to_string(&path).map_err(|failure| {
        RepositoryCommandFailure::PathUnreadable { path, reason: failure.to_string() }
    })?;
    coverage_fuzzing_tool::parse_pin(&text).map_err(|failure| {
        RepositoryCommandFailure::ToolFailed {
            program: program.to_owned(),
            reason: failure.to_string(),
        }
    })
}

/// Verifies one prepared coverage-fuzzing bundle and names the tool inside it.
fn verify_coverage_fuzzing_tool(
    arguments: &[String],
    working_directory: &Path,
    output: &mut dyn Write,
) -> Result<(), RepositoryCommandFailure> {
    let bundle = named_directory(arguments, "--bundle", VERIFY_FUZZING_TOOL_COMMAND)?;
    let pin = coverage_fuzzing_pin(working_directory, VERIFY_FUZZING_TOOL_COMMAND)?;
    let host = current_host();
    let executable = coverage_fuzzing_tool::verified(&bundle, &pin, &host).map_err(|failure| {
        RepositoryCommandFailure::ToolFailed {
            program: VERIFY_FUZZING_TOOL_COMMAND.to_owned(),
            reason: failure.to_string(),
        }
    })?;
    writeln!(output, "{}", executable.display())
        .map_err(|failure| RepositoryCommandFailure::OutputUnavailable(failure.to_string()))
}

/// Returns the host row this machine is.
fn current_host() -> String {
    format!(
        "{}-{}",
        std::env::consts::ARCH,
        if cfg!(target_os = "linux") {
            "unknown-linux-gnu"
        } else if cfg!(target_os = "macos") {
            "apple-darwin"
        } else {
            "pc-windows-msvc"
        }
    )
}

/// Acquires the pinned coverage tool, builds it twice, and bundles it.
///
/// This is the one command in this repository that fetches somebody else's
/// source, and the fetch is verified against the pin afterwards rather than
/// trusted from the reference that produced it.
fn prepare_coverage_fuzzing_tool(
    arguments: &[String],
    working_directory: &Path,
    output: &mut dyn Write,
) -> Result<(), RepositoryCommandFailure> {
    let destination = named_directory(arguments, "--output", PREPARE_FUZZING_TOOL_COMMAND)?;
    let host = named_value(arguments, "--host", PREPARE_FUZZING_TOOL_COMMAND)?;
    let pin = coverage_fuzzing_pin(working_directory, PREPARE_FUZZING_TOOL_COMMAND)?;
    let manifest =
        coverage_fuzzing_tool::prepare(&destination, &pin, &host).map_err(|failure| {
            RepositoryCommandFailure::ToolFailed {
                program: PREPARE_FUZZING_TOOL_COMMAND.to_owned(),
                reason: failure.to_string(),
            }
        })?;
    writeln!(output, "{} built twice to {}", pin.package, manifest.binary_sha256)
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
