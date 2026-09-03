//! Assertions for the repository-local quality gate.
//!
//! The gate itself runs the whole test suite, so these assertions never invoke
//! it end to end: they prove its contract from its committed text and from the
//! two refusals it reaches before any check runs.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

/// Repository path of the gate.
const GATE_PATH: &str = "scripts/quality";

/// Repository path of the pinned tool manifest.
const TOOL_MANIFEST_PATH: &str = "support/repository-tools.toml";

/// Repository path of the dependency policy.
const DEPENDENCY_POLICY_PATH: &str = "deny.toml";

/// Environment variable that names the advisory checkout.
const ADVISORY_VARIABLE: &str = "SLINGSHOT_RUSTSEC_ADVISORY_DATABASE_DIRECTORY";

/// Scope every Rust graph gate is run over.
const GATE_SCOPE: &str = "--locked --workspace --all-targets --all-features";

/// Commands the gate must run, in the order it runs them.
const REQUIRED_COMMANDS: &[&str] = &[
    "cargo fmt --all --check",
    "cargo check $CARGO_GATE_SCOPE",
    "cargo clippy $CARGO_GATE_SCOPE -- -D warnings",
    "cargo test $CARGO_GATE_SCOPE",
    "RUSTDOCFLAGS='-D warnings' cargo doc --locked --workspace --all-features --no-deps",
    "shellcheck --shell=sh scripts/*",
    "dependency-direction",
    "source-policy",
    "rustsec-advisory-pin",
    "cargo deny --offline --all-features check --disable-fetch",
];

/// Ways the gate must refuse to fetch or discover anything.
const REQUIRED_REFUSALS: &[&str] = &["--offline", "--disable-fetch", "--locked"];

/// The pinned tool manifest.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct RepositoryTools {
    /// Format identifier of the manifest.
    format: String,
    /// One entry per pinned external executable.
    tool: Vec<PinnedTool>,
    /// Where the pinned dependency-policy tool looks for the advisory database.
    advisory_database: AdvisoryDatabase,
}

/// One pinned external executable.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct PinnedTool {
    /// Name of the executable.
    name: String,
    /// Exact version the gate requires.
    version: String,
    /// Source the executable must come from.
    source: String,
    /// How the executable is installed.
    install: String,
    /// Command that reports a version, and the exact text it must contain.
    version_check: String,
}

/// Where the pinned dependency-policy tool looks for the advisory database.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct AdvisoryDatabase {
    /// Directory name the tool derives from the pinned database location.
    directory_name: String,
    /// Lock the tool creates while it reads, and the gate removes after.
    lock_file_name: String,
}

/// Returns the workspace root directory.
fn workspace_root() -> PathBuf {
    slingshot_development::locate_workspace_root(Path::new(env!("CARGO_MANIFEST_DIR")))
        .expect("the development crate lives inside the workspace")
}

/// Reads one repository file relative to the workspace root.
fn read_repository_file(relative: &str) -> String {
    let path = workspace_root().join(relative);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|failure| panic!("{} could not be read: {failure}", path.display()))
}

/// Runs the gate and returns what it produced.
fn run_gate(arguments: &[&str], advisory: Option<&str>) -> std::process::Output {
    let mut command = Command::new(workspace_root().join(GATE_PATH));
    command.current_dir(workspace_root()).args(arguments);
    match advisory {
        Some(checkout) => command.env(ADVISORY_VARIABLE, checkout),
        None => command.env_remove(ADVISORY_VARIABLE),
    };
    command.output().expect("the gate runs")
}

#[test]
fn the_gate_is_one_argument_free_executable_sequence() {
    let gate = workspace_root().join(GATE_PATH);
    assert!(gate.is_file(), "{} exists", gate.display());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&gate).expect("the gate is readable").permissions().mode();
        assert_ne!(mode & 0o111, 0, "the gate is executable");
    }
    let refused = run_gate(&["all"], Some("/nonexistent"));
    assert!(!refused.status.success(), "an argument is refused");
    let diagnostic = String::from_utf8_lossy(&refused.stderr);
    assert!(diagnostic.contains("takes no argument"), "{diagnostic}");
}

#[test]
fn the_gate_runs_every_required_command_over_the_whole_graph() {
    let gate = read_repository_file(GATE_PATH);
    for command in REQUIRED_COMMANDS {
        assert!(gate.contains(command), "the gate does not run {command}");
    }
    assert!(gate.contains(&format!("CARGO_GATE_SCOPE='{GATE_SCOPE}'")), "the scope drifted");
    for part in GATE_SCOPE.split(' ') {
        assert!(gate.contains(part), "the scope omits {part}");
    }
    for refusal in REQUIRED_REFUSALS {
        assert!(gate.contains(refusal), "the gate does not refuse to fetch through {refusal}");
    }
    assert!(!gate.contains("cargo update"), "the gate never changes the resolved graph");
    assert!(!gate.contains("git fetch"), "the gate never fetches");
    // A stage that names one script reports on the file a reader has just read
    // and leaves the release path unlinted, so naming one is itself the defect.
    assert!(
        !gate.contains("shellcheck --shell=sh scripts/quality"),
        "the scripts stage lints one script rather than every script"
    );
}

#[test]
fn the_gate_refuses_an_unnamed_advisory_checkout_before_any_check() {
    let refused = run_gate(&[], None);
    assert!(!refused.status.success(), "an unnamed checkout is refused");
    let diagnostic = String::from_utf8_lossy(&refused.stderr);
    assert!(diagnostic.contains(ADVISORY_VARIABLE), "{diagnostic}");
    let produced = String::from_utf8_lossy(&refused.stdout);
    assert!(produced.contains("repository tools"), "{produced}");
    for stage in ["formatting", "compilation", "lints", "tests", "documentation"] {
        assert!(!produced.contains(stage), "the gate reached {stage} before authenticating");
    }
}

#[test]
fn every_external_executable_is_pinned_to_an_exact_version_and_source() {
    let tools: RepositoryTools =
        toml::from_str(&read_repository_file(TOOL_MANIFEST_PATH)).expect("the manifest reads");
    assert_eq!(tools.format, "slingshot.repository-tools/1");
    assert!(!tools.tool.is_empty(), "the manifest pins at least one tool");
    for tool in &tools.tool {
        assert!(!tool.name.is_empty(), "every tool is named");
        assert!(!tool.source.is_empty(), "{} names its source", tool.name);
        assert!(!tool.install.is_empty(), "{} says how it is installed", tool.name);
        let (probe, required) =
            tool.version_check.split_once('|').expect("a check pairs a probe with its text");
        assert!(probe.contains(&tool.name) || required.contains(&tool.name), "{}", tool.name);
        assert!(
            required.contains(&tool.version),
            "{} does not check its pinned version",
            tool.name
        );
    }
    assert!(!tools.advisory_database.directory_name.is_empty());
    assert!(!tools.advisory_database.lock_file_name.is_empty());
    let gate = read_repository_file(GATE_PATH);
    assert!(gate.contains("lock-file-name"), "the gate removes the tool's own lock");
}

#[test]
fn the_dependency_policy_names_its_accepted_licenses_and_refuses_unknown_sources() {
    let policy: toml::Value =
        toml::from_str(&read_repository_file(DEPENDENCY_POLICY_PATH)).expect("the policy reads");
    let licenses = policy["licenses"]["allow"].as_array().expect("the policy allows licenses");
    assert!(!licenses.is_empty(), "the accepted license set is explicit");
    assert_eq!(policy["sources"]["unknown-registry"].as_str(), Some("deny"));
    assert_eq!(policy["sources"]["unknown-git"].as_str(), Some("deny"));
    assert_eq!(policy["advisories"]["yanked"].as_str(), Some("deny"));
    let registries = policy["sources"]["allow-registry"].as_array().expect("registries are named");
    assert_eq!(registries.len(), 1, "exactly one registry is accepted");
    assert!(
        policy["licenses"]["private"]["ignore"].as_bool().unwrap_or_default(),
        "the unpublished Slingshot packages are not given a license by this policy"
    );
}
