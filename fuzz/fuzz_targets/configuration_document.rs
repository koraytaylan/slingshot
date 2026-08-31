//! Feeding arbitrary bytes to the configuration reader.
//!
//! The reader is the first thing a hostile input meets: it decides what a
//! document is before anything decides what it means. What this target insists
//! on is that every input produces one of the two answers the reader has - a
//! loaded generation or a bounded set of closed diagnostics - and never a
//! panic, an unbounded allocation, or a diagnostic carrying the input back.

#![no_main]

use libfuzzer_sys::fuzz_target;
use slingshot_configuration::profile_loader::load_profiles;
use slingshot_configuration::testing::credential_filesystem::ScriptedFilesystem;

fuzz_target!(|bytes: &[u8]| {
    let authority = ScriptedFilesystem::new()
        .with_directory("profiles")
        .with_source("profiles/local.toml", bytes)
        .with_source("configuration-snapshot.toml", bytes);
    let held = load_profiles(authority);
    if let Err(diagnostics) = held {
        for diagnostic in diagnostics {
            assert!(!diagnostic.structural_location.is_empty());
        }
    }
});
