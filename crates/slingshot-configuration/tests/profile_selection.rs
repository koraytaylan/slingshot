//! Assertions for resolving one profile environment.
//!
//! The property that matters is what selection refuses to do. It never picks
//! for the caller: one name on its own is refused even when the root could
//! complete it, because completing it from a different source is how a command
//! ends up aimed at a server nobody named.
//!
//! The other property is what a refusal says. Two different missing names must
//! produce the same diagnostic, and no diagnostic may carry a requested name, a
//! candidate, or a source reference - otherwise the refusal enumerates the root
//! for whoever asked.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use slingshot_configuration::profile_loader::{
    ConfigurationDiagnostic, LoadedProfiles, load_profiles,
};
use slingshot_configuration::profile_selection::{RequestedSelection, resolve};
use slingshot_configuration::testing::credential_filesystem::ScriptedFilesystem;
use slingshot_domain::profile::{EnvironmentName, ProfileName};
use slingshot_domain::profile_authentication_contract::ConfigurationFailureCode;

/// Directory holding the committed profile directories.
const DIRECTORY_FIXTURES: &str = "../slingshot-test-support/fixtures/profile-directories";

/// Profile the fixture's selection document names.
const DEFAULT_PROFILE: &str = "alpha-site";

/// Environment the fixture's selection document names.
const DEFAULT_ENVIRONMENT: &str = "production";

/// Profile whose author address is cleartext and off loopback.
const WARNED_PROFILE: &str = "remote-site";

/// Environment of that profile.
const WARNED_ENVIRONMENT: &str = "staging";

/// Names no diagnostic may carry.
const SENTINELS: &[&str] =
    &["alpha-site", "zulu-site", "remote-site", "production", "staging", "profiles/"];

/// Returns the files one committed profile directory holds.
fn fixture_files(case: &str) -> BTreeMap<String, Vec<u8>> {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(DIRECTORY_FIXTURES).join(case);
    let mut files = BTreeMap::new();
    collect(&directory, &directory, &mut files);
    files
}

/// Collects every file below `directory`, keyed by its root-relative spelling.
fn collect(root: &Path, directory: &Path, files: &mut BTreeMap<String, Vec<u8>>) {
    for entry in std::fs::read_dir(directory).expect("the fixture directory reads") {
        let path = entry.expect("the entry reads").path();
        if path.is_dir() {
            collect(root, &path, files);
            continue;
        }
        let relative = path.strip_prefix(root).expect("the file is inside the fixture");
        let spelling = relative.to_str().expect("the path is text").replace('\\', "/");
        files.insert(spelling, std::fs::read(&path).expect("the file reads"));
    }
}

/// Returns the profiles the committed fixture holds.
fn loaded(with_selection: bool) -> LoadedProfiles {
    let mut authority = ScriptedFilesystem::new();
    let files = fixture_files("ordered");
    let mut inventory = String::from("format_version = 1\n");
    for (reference, bytes) in &files {
        if reference == "configuration-snapshot.toml" {
            continue;
        }
        if !with_selection && reference == "selection.toml" {
            continue;
        }
        authority = authority.with_source(reference, bytes);
        inventory.push_str(&format!(
            "\n[[sources]]\nreference = \"{reference}\"\nsha256 = \"{}\"\n",
            digest(bytes)
        ));
    }
    let authority = authority
        .with_source("configuration-snapshot.toml", inventory.as_bytes())
        .with_directory("profiles");
    load_profiles(authority).expect("the committed root loads")
}

/// Returns the lowercase hexadecimal digest of `bytes`.
fn digest(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Returns the request naming both `profile` and `environment`.
fn request(profile: &str, environment: &str) -> RequestedSelection {
    RequestedSelection {
        profile: Some(ProfileName::parse(profile).expect("the name is valid")),
        environment: Some(EnvironmentName::parse(environment).expect("the name is valid")),
    }
}

/// Refuses a diagnostic set carrying any name or reference.
fn refuse_sentinels(diagnostics: &[ConfigurationDiagnostic]) {
    let rendered = format!("{diagnostics:?}");
    for sentinel in SENTINELS {
        assert!(!rendered.contains(sentinel), "{rendered} carries {sentinel}");
    }
}

#[test]
fn an_explicit_pair_selects_exactly_what_it_names() {
    let profiles = loaded(true);
    let selection = resolve(&profiles, &request(WARNED_PROFILE, WARNED_ENVIRONMENT))
        .expect("the explicit pair resolves");
    assert_eq!(selection.profile_name().as_text(), WARNED_PROFILE);
    assert_eq!(selection.environment_name().as_text(), WARNED_ENVIRONMENT);
    assert_eq!(selection.namespace_key(), format!("{WARNED_PROFILE}/{WARNED_ENVIRONMENT}"));
    assert_eq!(selection.profile_source().as_text(), "profiles/mike.toml");
    assert_eq!(selection.selection_source().map(|source| source.as_text()), Some("selection.toml"));
}

#[test]
fn a_complete_default_selects_and_a_missing_one_refuses() {
    let profiles = loaded(true);
    let selection =
        resolve(&profiles, &RequestedSelection::default()).expect("the default pair resolves");
    assert_eq!(selection.profile_name().as_text(), DEFAULT_PROFILE);
    assert_eq!(selection.environment_name().as_text(), DEFAULT_ENVIRONMENT);

    let without = loaded(false);
    let diagnostics =
        resolve(&without, &RequestedSelection::default()).expect_err("no default is not a default");
    assert_eq!(diagnostics[0].code, ConfigurationFailureCode::SelectionIncomplete);
    refuse_sentinels(&diagnostics);
}

#[test]
fn one_name_on_its_own_is_never_completed_from_another_source() {
    let profiles = loaded(true);
    let only_profile = RequestedSelection {
        profile: Some(ProfileName::parse(DEFAULT_PROFILE).expect("the name is valid")),
        environment: None,
    };
    let only_environment = RequestedSelection {
        profile: None,
        environment: Some(EnvironmentName::parse(DEFAULT_ENVIRONMENT).expect("the name is valid")),
    };
    for partial in [only_profile, only_environment] {
        let diagnostics = resolve(&profiles, &partial).expect_err("one name is refused");
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, ConfigurationFailureCode::SelectionIncomplete);
        refuse_sentinels(&diagnostics);
    }
}

#[test]
fn two_differently_named_missing_selections_are_indistinguishable() {
    let profiles = loaded(true);
    let first = resolve(&profiles, &request("absent-site", DEFAULT_ENVIRONMENT))
        .expect_err("the profile is absent");
    let second = resolve(&profiles, &request("another-absent-site", "development"))
        .expect_err("the profile is absent");
    assert_eq!(first, second, "the refusals differ by the names that were asked for");
    assert_eq!(first[0].code, ConfigurationFailureCode::ProfileNotFound);
    refuse_sentinels(&first);

    let missing_environment = resolve(&profiles, &request(DEFAULT_PROFILE, "absent-environment"))
        .expect_err("the environment is absent");
    assert_eq!(missing_environment[0].code, ConfigurationFailureCode::EnvironmentNotFound);
    assert_ne!(missing_environment, first, "a missing environment is not a missing profile");
    refuse_sentinels(&missing_environment);
}

#[test]
fn only_an_opted_in_cleartext_selection_carries_a_warning() {
    let profiles = loaded(true);
    let warned = resolve(&profiles, &request(WARNED_PROFILE, WARNED_ENVIRONMENT))
        .expect("the cleartext selection resolves");
    assert!(warned.insecure_author_transport_warning().is_some());
    assert!(!warned.environment_of(&profiles).author_connection_target().is_protected());

    let protected = resolve(&profiles, &request(DEFAULT_PROFILE, DEFAULT_ENVIRONMENT))
        .expect("the protected selection resolves");
    assert!(protected.insecure_author_transport_warning().is_none());
    assert!(protected.environment_of(&profiles).author_connection_target().is_protected());
}

#[test]
fn the_same_pair_always_names_the_same_namespace() {
    let profiles = loaded(true);
    let first = resolve(&profiles, &request(DEFAULT_PROFILE, DEFAULT_ENVIRONMENT))
        .expect("the pair resolves");
    let again = resolve(&loaded(true), &request(DEFAULT_PROFILE, DEFAULT_ENVIRONMENT))
        .expect("the pair resolves again");
    assert_eq!(first, again, "one pair produced two selections");
    assert_eq!(first.namespace_key(), again.namespace_key());
    let other = resolve(&profiles, &request(WARNED_PROFILE, WARNED_ENVIRONMENT))
        .expect("another pair resolves");
    assert_ne!(first.namespace_key(), other.namespace_key());
}
