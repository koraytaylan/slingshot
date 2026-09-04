//! Assertions for the abstract supported-target matrix.
//!
//! Every row is evaluated through deterministic policy observations, so all
//! three targets are checked from one machine. A real observation is taken only
//! for the row that matches the current environment, and it is labelled
//! untrusted because nobody has attested the machine it describes.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use slingshot_development::supported_platform_matrix::{
    self, LINUX_TARGET_TRIPLE, MatrixFailure, PlatformObservations, SUPPORTED_TARGET_TRIPLES,
    SupportedPlatformMatrix, UNTRUSTED_OBSERVATION_LABEL, WINDOWS_TARGET_TRIPLE,
};

/// Repository path of the abstract supported-target manifest.
const MATRIX_PATH: &str = "support/platforms.toml";

/// Repository path of the capability inventory that names the same targets.
const CAPABILITY_POLICY_PATH: &str = "policy/workspace-capabilities.toml";

/// Directory holding the fixtures this test evaluates.
const FIXTURE_DIRECTORY: &str =
    "crates/slingshot-development/tests/fixtures/supported-platform-matrix";

/// Fixture holding the deterministic policy observations.
const OBSERVATION_FIXTURE: &str = "policy-observations.toml";

/// Rejected matrix fixtures and the reason each one is refused.
const REJECTED_MATRICES: &[&str] = &[
    "rejected-access-control-list-only-remote-protection.toml",
    "rejected-aggregate-success.toml",
    "rejected-concrete-provider.toml",
    "rejected-cross-compiled-host-class.toml",
    "rejected-duplicate-target.toml",
    "rejected-family-fallback.toml",
    "rejected-linker-digest.toml",
    "rejected-missing-executable-suffix.toml",
    "rejected-placeholder-capability.toml",
    "rejected-runner-image.toml",
    "rejected-software-development-kit-digest.toml",
    "rejected-unsupported-target.toml",
    "rejected-windows-without-remote-client-rejection.toml",
    "rejected-wrong-windows-archive-profile.toml",
];

/// Capability every Windows named-pipe server creation must carry.
const WINDOWS_REMOTE_CLIENT_CAPABILITY: &str = "named-pipe-reject-remote-clients";

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

/// Reads and parses the committed supported-target manifest.
fn committed_matrix() -> SupportedPlatformMatrix {
    supported_platform_matrix::parse_matrix(&read_repository_file(MATRIX_PATH))
        .expect("the committed matrix is a valid document")
}

/// Reads the deterministic policy observations.
fn committed_observations() -> PlatformObservations {
    let text = read_repository_file(&format!("{FIXTURE_DIRECTORY}/{OBSERVATION_FIXTURE}"));
    toml::from_str(&text).expect("the observation fixture is a valid document")
}

#[test]
fn the_committed_matrix_declares_the_exact_abstract_rows() {
    let matrix = committed_matrix();
    assert_eq!(supported_platform_matrix::validate_matrix(&matrix), Vec::<String>::new());
    let declared: Vec<&str> = matrix.target.iter().map(|row| row.triple.as_str()).collect();
    assert_eq!(declared, SUPPORTED_TARGET_TRIPLES);
    let unique: BTreeSet<&str> = declared.iter().copied().collect();
    assert_eq!(unique.len(), declared.len(), "a triple is declared twice");
}

#[test]
fn every_row_pins_its_exact_executable_and_archive_layout() {
    let matrix = committed_matrix();
    let expected = [
        ("x86_64-unknown-linux-gnu", "", "tar.gz", "slingshot"),
        ("aarch64-apple-darwin", "", "tar.gz", "slingshot"),
    ];
    assert_eq!(matrix.target.len(), expected.len(), "a row is unpinned");
    for (row, (triple, suffix, profile, executable)) in matrix.target.iter().zip(expected) {
        assert_eq!(row.triple, triple);
        assert_eq!(row.executable_stem, "slingshot");
        assert_eq!(row.executable_suffix, suffix);
        assert_eq!(row.archive_profile, profile);
        assert_eq!(row.native_smoke_mode, "direct");
        assert_eq!(row.archive_members, vec![executable, "LICENSE", "SHA256SUMS"]);
    }
}

#[test]
fn a_rows_rules_outlive_the_matrix_claiming_that_row() {
    // Windows is not a supported row today, and the rules it obeys are still
    // here: they belong to the row rather than to the matrix, so the row can
    // return without anyone reconstructing what it was held to.
    let required = supported_platform_matrix::required_capabilities(WINDOWS_TARGET_TRIPLE);
    assert!(
        required.contains(&WINDOWS_REMOTE_CLIENT_CAPABILITY),
        "the Windows row's remote-client rule went with the row: {required:?}"
    );
    assert!(
        !supported_platform_matrix::required_capabilities(LINUX_TARGET_TRIPLE)
            .contains(&WINDOWS_REMOTE_CLIENT_CAPABILITY),
        "a rule that belongs to one row reached another"
    );
}

#[test]
fn deterministic_policy_observations_are_accepted_or_refused_as_recorded() {
    let matrix = committed_matrix();
    let observations = committed_observations();
    let mut evaluated = BTreeSet::new();
    for observation in &observations.observation {
        let row = matrix
            .target
            .iter()
            .find(|candidate| candidate.triple == observation.triple)
            .unwrap_or_else(|| panic!("{} names no supported row", observation.name));
        let violations = supported_platform_matrix::evaluate_observation(row, observation);
        assert_eq!(
            violations.is_empty(),
            observation.accepted,
            "{}: {violations:?}",
            observation.name
        );
        evaluated.insert(observation.triple.clone());
    }
    let expected: BTreeSet<String> =
        SUPPORTED_TARGET_TRIPLES.iter().map(|triple| (*triple).to_owned()).collect();
    assert_eq!(evaluated, expected, "every abstract row has deterministic coverage");
}

#[test]
fn every_recorded_rejected_matrix_is_refused() {
    for name in REJECTED_MATRICES {
        let text = read_repository_file(&format!("{FIXTURE_DIRECTORY}/{name}"));
        let refused = match supported_platform_matrix::parse_matrix(&text) {
            Err(_) => true,
            Ok(matrix) => !supported_platform_matrix::validate_matrix(&matrix).is_empty(),
        };
        assert!(refused, "{name} must be refused");
    }
}

#[test]
fn a_current_environment_observation_accepts_at_most_its_own_row() {
    let text = read_repository_file(MATRIX_PATH);
    let matrix = committed_matrix();
    let current = supported_platform_matrix::current_target_triple();
    for triple in SUPPORTED_TARGET_TRIPLES {
        let observed =
            supported_platform_matrix::observe_current_native(&matrix, text.as_bytes(), triple);
        match (current, observed) {
            (Some(matched), Ok(observation)) => {
                assert_eq!(observation.triple, matched);
                assert_eq!(observation.triple, *triple);
                assert_eq!(observation.label, UNTRUSTED_OBSERVATION_LABEL);
                assert_eq!(observation.operating_system, std::env::consts::OS);
                assert_eq!(observation.architecture, std::env::consts::ARCH);
                assert_eq!(
                    observation.matrix_digest,
                    supported_platform_matrix::matrix_digest(text.as_bytes())
                );
            }
            (Some(matched), Err(failure)) => {
                assert_ne!(matched, *triple, "the matching row must be observable");
                assert!(matches!(failure, MatrixFailure::NotCurrentRow { .. }), "{failure}");
            }
            (None, observed) => {
                assert!(observed.is_err(), "an unsupported environment observes no row");
            }
        }
    }
}

#[test]
fn the_matrix_target_set_equals_the_target_conditioned_capability_rows() {
    let matrix = committed_matrix();
    let policy = read_repository_file(CAPABILITY_POLICY_PATH);
    let document: toml::Value = toml::from_str(&policy).expect("the capability policy parses");
    let supported: BTreeSet<&str> = document["supported-targets"]
        .as_array()
        .expect("the capability policy names its supported targets")
        .iter()
        .filter_map(toml::Value::as_str)
        .collect();
    let declared: BTreeSet<&str> = matrix.target.iter().map(|row| row.triple.as_str()).collect();
    assert_eq!(declared, supported);
}
