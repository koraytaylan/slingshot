//! The configuration crate's own boundary, proved without the composed harness.
//!
//! This file deliberately imports nothing from the outermost tooling crate. The
//! composed transcript belongs there; what belongs here is the contract this
//! crate is responsible for on its own - that a generation is accepted whole or
//! not at all, that a selection is explicit, and that neither reports anything a
//! reader was not already entitled to.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use slingshot_configuration::profile_loader::load_profiles;
use slingshot_configuration::profile_selection::{RequestedSelection, resolve};
use slingshot_configuration::testing::credential_filesystem::ScriptedFilesystem;
use slingshot_domain::profile::{EnvironmentName, ProfileName};
use slingshot_domain::profile_authentication_contract::ConfigurationFailureCode;

/// Directory holding the committed profile directory.
const PROFILE_FIXTURES: &str = "../slingshot-test-support/fixtures/profile-directories/ordered";

/// Values no diagnostic this crate produces may carry.
const SENTINELS: &[&str] = &["not-a-real-password", "credentials/alpha.json", "alpha-site"];

/// Returns the files the committed profile directory holds.
fn profile_files() -> BTreeMap<String, Vec<u8>> {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(PROFILE_FIXTURES);
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
        files.insert(
            relative.to_str().expect("the path is text").replace('\\', "/"),
            std::fs::read(&path).expect("the file reads"),
        );
    }
}

/// Returns a scripted root holding the committed generation.
fn scripted() -> ScriptedFilesystem {
    let mut authority = ScriptedFilesystem::new();
    for (reference, bytes) in profile_files() {
        authority = authority.with_source(&reference, &bytes);
    }
    authority.with_directory("profiles")
}

#[test]
fn a_generation_is_accepted_whole_and_a_selection_is_explicit() {
    let loaded = load_profiles(scripted()).expect("the committed generation loads");
    assert!(loaded.profiles().len() > 1, "the fixture holds one profile");
    let selection = resolve(
        &loaded,
        &RequestedSelection {
            profile: Some(ProfileName::parse("remote-site").expect("the name is valid")),
            environment: Some(EnvironmentName::parse("staging").expect("the name is valid")),
        },
    )
    .expect("the explicit pair resolves");
    assert_eq!(selection.namespace_key(), "remote-site/staging");

    let partial = resolve(
        &loaded,
        &RequestedSelection {
            profile: Some(ProfileName::parse("remote-site").expect("the name is valid")),
            environment: None,
        },
    )
    .expect_err("one name alone is refused");
    assert_eq!(partial[0].code, ConfigurationFailureCode::SelectionIncomplete);
}

#[test]
fn nothing_this_crate_reports_carries_a_source_it_read() {
    let loaded = load_profiles(scripted()).expect("the committed generation loads");
    let refusal = resolve(
        &loaded,
        &RequestedSelection {
            profile: Some(ProfileName::parse("absent-site").expect("the name is valid")),
            environment: Some(EnvironmentName::parse("production").expect("the name is valid")),
        },
    )
    .expect_err("an absent profile is refused");
    let rendered = format!("{refusal:?}");
    for sentinel in SENTINELS {
        assert!(!rendered.contains(sentinel), "{rendered} carries {sentinel}");
    }

    let broken = load_profiles(ScriptedFilesystem::new().with_directory("profiles"))
        .expect_err("an empty root is not a generation");
    let rendered = format!("{broken:?}");
    for sentinel in SENTINELS {
        assert!(!rendered.contains(sentinel), "{rendered} carries {sentinel}");
    }
}

#[test]
fn nothing_here_reaches_for_the_outermost_crate() {
    let crate_directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source =
        std::fs::read_to_string(crate_directory.join("tests/profile_authentication_boundaries.rs"))
            .expect("this file reads");
    let importing: Vec<&str> = source
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("use ") || line.starts_with("extern crate "))
        .filter(|line| line.contains("development"))
        .collect();
    assert!(importing.is_empty(), "a focused boundary test imports the harness: {importing:?}");

    let manifest =
        std::fs::read_to_string(crate_directory.join("Cargo.toml")).expect("the manifest reads");
    let declared: Vec<&str> = manifest
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("slingshot-development"))
        .collect();
    assert!(declared.is_empty(), "this crate can reach the harness at all: {declared:?}");
}
