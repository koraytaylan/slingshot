//! Every seed the fuzzer starts from, replayed through the production reader.
//!
//! A corpus that nothing checks rots: seeds stop parsing, stop being
//! interesting, or stop existing, and nobody finds out until a fuzzing run that
//! was supposed to take hours ends in seconds. So every seed is replayed here
//! by an ordinary test, on every platform this product supports, in the gate
//! that runs on every change.
//!
//! What is asserted is what the fuzz target asserts: one of the two answers the
//! reader has, bounded diagnostics in the closed vocabulary, and nothing of the
//! input carried back out. A diagnostic that quoted the document would hand
//! whoever reads the log the contents of somebody's configuration.

use std::path::PathBuf;

use slingshot_configuration::profile_loader::load_profiles;
use slingshot_configuration::testing::credential_filesystem::ScriptedFilesystem;

/// Where the seeds live.
const CORPUS: &str = "../../fuzz/corpus/configuration_document";

/// How many directories separate this crate from the workspace root.
const CRATE_DEPTH: usize = 2;

/// The fewest seeds a corpus worth keeping holds.
const LEAST_SEEDS: usize = 16;

/// A value no diagnostic may carry back out.
const SECRET_SENTINEL: &str = "s3ntinel-password-9f2a4c";

/// Returns the workspace root.
fn workspace_root() -> PathBuf {
    let mut root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for _ in 0..CRATE_DEPTH {
        root = root.parent().expect("the crate is inside the workspace").to_path_buf();
    }
    root
}

/// Returns every seed, by name and bytes.
fn seeds() -> Vec<(String, Vec<u8>)> {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(CORPUS);
    let mut held: Vec<(String, Vec<u8>)> = std::fs::read_dir(&directory)
        .unwrap_or_else(|failure| panic!("{} could not be read: {failure}", directory.display()))
        .filter_map(Result::ok)
        .map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let bytes = std::fs::read(entry.path()).expect("the seed reads");
            (name, bytes)
        })
        .collect();
    held.sort();
    held
}

/// Returns what the reader answers for one seed.
fn read(
    bytes: &[u8],
) -> Result<(), Vec<slingshot_configuration::profile_loader::ConfigurationDiagnostic>> {
    let authority = ScriptedFilesystem::new()
        .with_directory("profiles")
        .with_source("profiles/local.toml", bytes)
        .with_source("configuration-snapshot.toml", bytes);
    load_profiles(authority).map(|_| ())
}

#[test]
fn the_corpus_holds_enough_seeds_to_be_worth_starting_from() {
    let held = seeds();
    assert!(held.len() >= LEAST_SEEDS, "a corpus of {} is barely a corpus", held.len());
    let named: std::collections::BTreeSet<&str> =
        held.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(named.len(), held.len(), "a seed is committed twice");
}

#[test]
fn every_seed_produces_one_of_the_two_answers_the_reader_has() {
    for (name, bytes) in seeds() {
        let held = read(&bytes);
        match held {
            Ok(()) => {}
            Err(diagnostics) => {
                assert!(!diagnostics.is_empty(), "{name} was refused for no stated reason");
                for diagnostic in diagnostics {
                    assert!(
                        !diagnostic.structural_location.is_empty(),
                        "{name} produced a diagnostic that says nothing about where"
                    );
                }
            }
        }
    }
}

#[test]
fn reading_one_seed_twice_answers_the_same_way_twice() {
    for (name, bytes) in seeds() {
        let first = read(&bytes).map_err(|held| held.len());
        let again = read(&bytes).map_err(|held| held.len());
        assert_eq!(first, again, "{name} answered differently the second time");
    }
}

#[test]
fn no_diagnostic_carries_the_document_or_anything_in_it_back_out() {
    for (name, bytes) in seeds() {
        let Err(diagnostics) = read(&bytes) else {
            continue;
        };
        let rendered = format!("{diagnostics:?}");
        assert!(!rendered.contains(SECRET_SENTINEL), "{name} carried a secret into a diagnostic");
        for excerpt in ["format_version", "base_address", "password", "user_name"] {
            assert!(
                !rendered.contains(excerpt),
                "{name} carried {excerpt} out of the document it read"
            );
        }
    }
}

#[test]
fn the_target_that_consumes_this_corpus_exists_and_names_it() {
    let target = workspace_root().join("fuzz/fuzz_targets/configuration_document.rs");
    let held = std::fs::read_to_string(&target).expect("the target is committed");
    assert!(held.contains("load_profiles"), "the target drives the production reader");
    assert!(held.contains("#![no_main]"), "and is built as a fuzz target");
    let manifest = std::fs::read_to_string(workspace_root().join("fuzz/Cargo.toml"))
        .expect("the fuzz workspace is committed");
    assert!(manifest.contains("configuration_document"), "the target is declared");
    let workspace = std::fs::read_to_string(workspace_root().join("Cargo.toml"))
        .expect("the workspace manifest is committed");
    assert!(
        workspace.contains("exclude = [\"fuzz\"]"),
        "the ordinary build does not depend on a nightly toolchain"
    );
}
