//! Assertions for resolving the configuration root from the account database.
//!
//! Two independent things are proved here. The rules above the resolver - what
//! an empty, relative, non-text, unavailable, or ambiguous answer becomes - are
//! proved on any host through a fake that returns answers this machine cannot
//! produce, so every supported row's behavior is checked without needing that
//! row. And the interfaces each row actually consults are compared with a
//! fixture, so a silent switch to an environment-derived answer is visible even
//! on a host that cannot run that row.
//!
//! The current row also runs its real resolver, twice: once here and once in a
//! child process carrying decoy values for every environment variable the
//! contract says is ignored. That observation is explicitly untrusted and
//! claims nothing about any other row.

use std::path::PathBuf;

use serde::Deserialize;
use slingshot_configuration::configuration_root::{
    AccountIdentity, AccountProfile, AccountResolver, ConfigurationRoot, ConfigurationRootFailure,
    OperatingSystemAccountResolver,
};
use slingshot_domain::profile_authentication_contract::{
    ConfigurationFailureCode, ProfileAuthenticationContract,
};

/// Fixture that records the account sources and the answer categories.
const POLICY_FIXTURE: &str = "tests/fixtures/configuration-root/account-policy.toml";

/// Source file the interface scans read.
const ROOT_SOURCE: &str = "src/configuration_root.rs";

/// Directory holding this crate's production modules.
const SOURCE_DIRECTORY: &str = "src";

/// Repository matrix of every supported target, relative to this crate.
const PLATFORM_MATRIX: &str = "../../support/platforms.toml";

/// Environment variable that tells a re-executed child which side it is on.
const CHILD_MARKER: &str = "SLINGSHOT_CONFIGURATION_ROOT_CHILD";

/// Line prefix a re-executed child reports its resolved root with.
const REPORTED_ROOT: &str = "resolved-root=";

/// Value a decoy environment variable carries.
const DECOY_HOME: &str = "/nonexistent-decoy-home";

/// Label every current-environment observation carries.
const UNTRUSTED_LABEL: &str = "untrusted_current_native_observation";

/// The account policy fixture.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AccountPolicy {
    /// Format identifier of the fixture.
    format: String,
    /// One entry per supported target.
    row: Vec<PolicyRow>,
    /// One entry per answer category.
    answer: Vec<PolicyAnswer>,
}

/// What one supported target resolves its account from.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PolicyRow {
    /// Target triple the row describes.
    target: String,
    /// Kind of identity the row samples.
    identity_kind: String,
    /// Interface that answers which account this process runs as.
    account_source: String,
    /// Interface that answers where that account's home is.
    home_source: String,
}

/// One answer category and the outcome it produces.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PolicyAnswer {
    /// Name of the category.
    name: String,
    /// Outcome the category produces.
    outcome: String,
}

/// An account resolver that answers exactly what a test asks it to.
struct ScriptedResolver {
    /// Answer this resolver produces.
    answer: Result<AccountProfile, ConfigurationRootFailure>,
}

impl AccountResolver for ScriptedResolver {
    fn resolve(&self) -> Result<AccountProfile, ConfigurationRootFailure> {
        self.answer.clone()
    }
}

/// Returns the directory this crate's manifest lives in.
fn crate_directory() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Reads one file relative to this crate.
fn read_crate_file(relative: &str) -> String {
    let path = crate_directory().join(relative);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|failure| panic!("{} could not be read: {failure}", path.display()))
}

/// Returns the committed account policy.
fn policy() -> AccountPolicy {
    toml::from_str(&read_crate_file(POLICY_FIXTURE)).expect("the account policy reads")
}

/// Returns the failure one named category produces.
fn scripted_failure(code: ConfigurationFailureCode) -> ScriptedResolver {
    ScriptedResolver { answer: Err(ConfigurationRootFailure::at(code, "configuration_root")) }
}

/// Returns a resolver answering with `home` for a Unix account.
fn scripted_home(home: PathBuf) -> ScriptedResolver {
    let identity = AccountIdentity::UnixUser(0);
    ScriptedResolver { answer: Ok(AccountProfile { identity, home }) }
}

/// Returns a home whose bytes are not Unicode text.
#[cfg(unix)]
fn home_that_is_not_unicode() -> PathBuf {
    use std::os::unix::ffi::OsStringExt;

    PathBuf::from(std::ffi::OsString::from_vec(vec![b'/', 0xff, b'h']))
}

/// Returns a home whose bytes are not Unicode text.
#[cfg(not(unix))]
fn home_that_is_not_unicode() -> PathBuf {
    use std::os::windows::ffi::OsStringExt;

    PathBuf::from(std::ffi::OsString::from_wide(&[0x005C, 0xD800, 0x0068]))
}

/// Returns the resolver one answer category is modelled with.
fn resolver_for(category: &str) -> ScriptedResolver {
    match category {
        "absolute-home" => scripted_home(absolute_home()),
        "empty-home" => scripted_home(PathBuf::new()),
        "relative-home" => scripted_home(PathBuf::from("relative/home")),
        "home-not-unicode" => scripted_home(home_that_is_not_unicode()),
        "account-unavailable" => {
            scripted_failure(ConfigurationFailureCode::ConfigurationAccountUnavailable)
        }
        "ambiguous-account" => {
            scripted_failure(ConfigurationFailureCode::ConfigurationHomeAmbiguous)
        }
        "unsupported-platform" => scripted_failure(ConfigurationFailureCode::UnsupportedPlatform),
        other => panic!("the fixture names the unknown category {other}"),
    }
}

/// Returns every target the workspace supports, in name order.
///
/// The list is read from the repository's own supported-platform matrix rather
/// than repeated here, so a row added there is a row this fixture must describe.
fn supported_targets() -> Vec<String> {
    let matrix = std::fs::read_to_string(crate_directory().join(PLATFORM_MATRIX))
        .expect("the supported-platform matrix reads");
    let mut targets: Vec<String> = matrix
        .lines()
        .filter_map(|line| line.trim().strip_prefix("triple = \""))
        .filter_map(|rest| rest.strip_suffix('"'))
        .map(str::to_owned)
        .collect();
    targets.sort();
    assert!(!targets.is_empty(), "the matrix declares no target");
    targets
}

/// Returns an absolute home this host agrees is absolute.
fn absolute_home() -> PathBuf {
    let mut home = std::env::temp_dir();
    home.push("slingshot-account-home");
    home
}

#[test]
fn every_recorded_answer_category_produces_its_stable_outcome() {
    let policy = policy();
    assert_eq!(policy.format, "slingshot.account-policy/1");
    for answer in policy.answer {
        let resolved = ConfigurationRoot::resolve(&resolver_for(&answer.name));
        match resolved {
            Ok(root) => {
                assert_eq!(answer.outcome, "accepted", "{} was accepted", answer.name);
                assert!(root.path().starts_with(absolute_home()), "{}", root.path().display());
            }
            Err(failure) => {
                assert_eq!(failure.code.code(), answer.outcome, "{}", answer.name);
            }
        }
    }
}

#[test]
fn the_root_is_the_account_home_plus_the_contract_components() {
    let literals = &ProfileAuthenticationContract::embedded().literals;
    let home = absolute_home();
    let root = ConfigurationRoot::resolve(&scripted_home(home.clone())).expect("the home resolves");
    let mut expected = home.clone();
    for component in &literals.configuration_root_components {
        expected.push(component);
    }
    assert_eq!(root.path(), expected);
    assert_eq!(root.traversal_origin(), home);
    assert_eq!(root.identity(), &AccountIdentity::UnixUser(0));
    assert_eq!(root.profile_directory(), expected.join(&literals.profile_directory_name));
    assert_eq!(root.selection_file(), expected.join(&literals.selection_file_name));
    assert_eq!(
        root.commit_inventory_file(),
        expected.join(&literals.configuration_snapshot_file_name)
    );
}

#[test]
fn every_supported_row_resolves_through_the_interfaces_the_fixture_records() {
    let policy = policy();
    let mut recorded: Vec<&str> = policy.row.iter().map(|row| row.target.as_str()).collect();
    recorded.sort_unstable();
    assert_eq!(recorded, supported_targets(), "the fixture omits or invents a supported row");
    let source = read_crate_file(ROOT_SOURCE);
    for row in &policy.row {
        assert!(
            source.contains(&row.account_source),
            "{} resolves its account through something else",
            row.target
        );
        assert!(
            source.contains(&row.home_source),
            "{} resolves its home through something else",
            row.target
        );
    }
    let kinds: Vec<&str> = policy.row.iter().map(|row| row.identity_kind.as_str()).collect();
    assert!(kinds.contains(&"unix-effective-user"));
    assert!(kinds.contains(&"windows-process-token-user"));
}

#[test]
fn no_module_reads_an_environment_variable_to_find_the_root() {
    let ignored = &ProfileAuthenticationContract::embedded().literals.ignored_home_variables;
    let code = executable_lines(&read_crate_file(ROOT_SOURCE));
    for variable in ignored {
        assert!(!code.contains(variable), "the resolver names {variable}");
    }
    for path in production_modules() {
        let code = executable_lines(&std::fs::read_to_string(&path).expect("the module reads"));
        assert!(!code.contains("std::env"), "{} reads the environment", path.display());
        assert!(!code.contains("current_dir"), "{} reads the working directory", path.display());
    }
}

/// Returns the lines of one module that are code rather than documentation.
///
/// The module documentation names every ignored variable on purpose, so a scan
/// that included it would find exactly what it is looking for.
fn executable_lines(source: &str) -> String {
    source
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<&str>>()
        .join("\n")
}

/// Returns every production module of this crate.
fn production_modules() -> Vec<PathBuf> {
    let mut pending = vec![crate_directory().join(SOURCE_DIRECTORY)];
    let mut modules = Vec::new();
    while let Some(entry) = pending.pop() {
        for child in std::fs::read_dir(&entry).expect("the source directory reads") {
            let path = child.expect("the entry reads").path();
            if path.is_dir() {
                pending.push(path);
            } else {
                modules.push(path);
            }
        }
    }
    modules
}

#[test]
fn the_explicit_test_root_is_never_used_by_a_production_module() {
    let declarations: usize = production_modules()
        .iter()
        .map(|path| {
            let text = std::fs::read_to_string(path).expect("the module reads");
            executable_lines(&text).matches("at_explicit_path").count()
        })
        .sum();
    assert_eq!(declarations, 1, "the explicit test root is reachable from a production module");
}

#[test]
fn no_environment_variable_can_move_the_current_row_root() {
    let Ok(native) = ConfigurationRoot::resolve(&OperatingSystemAccountResolver) else {
        return;
    };
    let rendered = native.path().display().to_string();
    if std::env::var_os(CHILD_MARKER).is_some() {
        println!("{REPORTED_ROOT}{rendered}");
        return;
    }
    let executable = std::env::current_exe().expect("the test binary is on disk");
    let mut command = std::process::Command::new(executable);
    command.args([
        "no_environment_variable_can_move_the_current_row_root",
        "--exact",
        "--nocapture",
    ]);
    command.env(CHILD_MARKER, "1");
    for variable in &ProfileAuthenticationContract::embedded().literals.ignored_home_variables {
        command.env(variable, DECOY_HOME);
    }
    let produced = command.output().expect("the child runs");
    let text = String::from_utf8_lossy(&produced.stdout);
    let reported = text
        .lines()
        .find_map(|line| line.strip_prefix(REPORTED_ROOT))
        .unwrap_or_else(|| panic!("the child reported no root: {text}"));
    assert_eq!(reported, rendered, "a decoy environment moved the root");
    println!("{UNTRUSTED_LABEL}: {rendered}");
}
