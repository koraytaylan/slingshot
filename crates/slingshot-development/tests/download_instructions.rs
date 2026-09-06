//! Keeps the public release instructions tied to the verifier they describe.
//!
//! A release archive without a usable verification command leaves its
//! attestation as a claim. This test makes the documented command and the
//! verifier's real interface one contract: no accepted option is omitted, and
//! no documented option is invented.

use std::path::{Path, PathBuf};

use slingshot_development::supported_platform_matrix::{self, SupportedPlatformMatrix};

const RELEASE_DOCUMENT: &str = "docs/RELEASES.md";
const VERIFIER: &str = "scripts/verify_release_artifacts";
const VERIFIER_OPTIONS: &[&str] =
    &["--archive", "--attestation-bundle", "--evidence", "--source-commit", "--cache-sha256"];

fn workspace_root() -> PathBuf {
    slingshot_development::locate_workspace_root(Path::new(env!("CARGO_MANIFEST_DIR")))
        .expect("the development crate lives inside the workspace")
}

fn read(relative: &str) -> String {
    let path = workspace_root().join(relative);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|failure| panic!("{} could not be read: {failure}", path.display()))
}

fn supported_matrix() -> SupportedPlatformMatrix {
    supported_platform_matrix::parse_matrix(&read("support/platforms.toml"))
        .expect("the supported platform manifest is valid")
}

fn verifier_options(script: &str) -> Vec<String> {
    script
        .lines()
        .filter_map(|line| {
            let option = line.trim().split_once(')')?.0;
            option.starts_with("--").then(|| option.to_owned())
        })
        .collect()
}

#[test]
fn release_instructions_describe_every_supported_row_and_its_evidence() {
    let document = read(RELEASE_DOCUMENT);
    assert!(document.contains("attestation.jsonl"));
    assert!(document.contains("evidence.toml"));
    assert!(document.contains("offline"));
    assert!(document.contains("reaches nothing"));
    for row in supported_matrix().target {
        assert!(document.contains(&row.triple), "the instructions omit {}", row.triple);
    }
}

#[test]
fn documented_verification_command_matches_the_verifier_interface() {
    let document = read(RELEASE_DOCUMENT);
    let accepted = verifier_options(&read(VERIFIER));
    assert_eq!(
        accepted,
        VERIFIER_OPTIONS.iter().map(|option| (*option).to_owned()).collect::<Vec<_>>()
    );
    for option in VERIFIER_OPTIONS {
        assert!(document.contains(option), "the command omits required option {option}");
    }
    for option in accepted {
        assert!(document.contains(&option), "the command documents unknown option {option}");
    }
}
