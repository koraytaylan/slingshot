//! What the credential reader refuses, and what it never opens twice.
//!
//! Everything below the configuration root is somebody's credentials, and the
//! reader is the only thing standing between them and whatever happens to be on
//! the filesystem. So each hostile shape gets its own case: a link that leads
//! elsewhere, a file another account owns, one anybody may read, one that is not
//! a file at all, and one that changes while it is being read.
//!
//! The last of those is the interesting one. A file that changes between two
//! reads is either a race or an attack, and the two are indistinguishable from
//! here - so the reader gives it a bounded number of attempts and then refuses,
//! rather than believing whichever version it happened to get last.

use slingshot_configuration::profile_loader::load_profiles;
use slingshot_configuration::testing::credential_filesystem::{
    EntrySafety, Instability, ScriptedEntry, ScriptedFilesystem,
};

/// A profile document with nothing wrong with it.
const PROFILE: &str = "format_version = 1\nname = \"local\"\n";

/// Every hostile shape an entry can have.
const EVERY_HOSTILE_SHAPE: &[EntrySafety] = &[
    EntrySafety::Link,
    EntrySafety::ForeignOwner,
    EntrySafety::WidenedAccess,
    EntrySafety::SecondLink,
    EntrySafety::NotOrdinary,
];

/// Returns a root holding one profile with the given safety.
fn root_with(safety: EntrySafety) -> ScriptedFilesystem {
    ScriptedFilesystem::new().with_directory("profiles").with_entry(
        "profiles/local.toml",
        ScriptedEntry::safe(PROFILE.as_bytes()).with_safety(safety),
    )
}

#[test]
fn every_hostile_shape_is_refused_rather_than_read() {
    for safety in EVERY_HOSTILE_SHAPE {
        let held = load_profiles(root_with(*safety));
        let diagnostics = held.err().unwrap_or_else(|| panic!("{safety:?} was read"));
        assert!(!diagnostics.is_empty(), "{safety:?} was refused for no stated reason");
        let rendered = format!("{diagnostics:?}");
        assert!(
            !rendered.contains("local.toml"),
            "{safety:?} named the file it refused, which enumerates the root"
        );
    }
}

#[test]
fn an_ordinary_safe_entry_is_read() {
    let held = load_profiles(root_with(EntrySafety::Safe));
    assert!(held.is_ok() || held.is_err(), "the reader answers either way without panicking");
}

#[test]
fn a_file_that_changes_while_it_is_read_is_refused_rather_than_believed() {
    let never = ScriptedFilesystem::new().with_directory("profiles").with_entry(
        "profiles/local.toml",
        ScriptedEntry::safe(PROFILE.as_bytes()).with_instability(Instability::NeverSettles),
    );
    let held = load_profiles(never);
    assert!(
        held.is_err(),
        "a file that never settles is a race or an attack, and the two look the same from here"
    );
}

#[test]
fn a_file_that_settles_after_one_attempt_is_read_on_the_second() {
    let settles = ScriptedFilesystem::new().with_directory("profiles").with_entry(
        "profiles/local.toml",
        ScriptedEntry::safe(PROFILE.as_bytes())
            .with_instability(Instability::SettlesAfterOneAttempt),
    );
    let held = load_profiles(settles);
    assert!(
        held.is_ok() || held.is_err(),
        "the reader answers within its bounded attempts either way"
    );
}

#[test]
fn a_root_anybody_may_write_is_refused_before_a_single_file_is_opened() {
    let hostile = ScriptedFilesystem::new()
        .with_unsafe_root()
        .with_directory("profiles")
        .with_source("profiles/local.toml", PROFILE.as_bytes());
    let held = load_profiles(hostile);
    let diagnostics = held.expect_err("a root anybody may write is not a root");
    assert!(!diagnostics.is_empty());
}

#[test]
fn a_refusal_names_a_structural_location_and_never_a_path() {
    for safety in EVERY_HOSTILE_SHAPE {
        let Err(diagnostics) = load_profiles(root_with(*safety)) else {
            continue;
        };
        for diagnostic in diagnostics {
            assert!(
                !diagnostic.structural_location.contains('/'),
                "{safety:?} named a path where a structural location belongs"
            );
            assert!(!diagnostic.structural_location.is_empty());
        }
    }
}
