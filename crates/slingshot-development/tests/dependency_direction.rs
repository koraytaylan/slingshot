//! Assertions for the workspace dependency-direction check.
//!
//! The evaluator is exercised through small hand-authored metadata documents
//! rather than through the live workspace alone, so every rejection is proved
//! independently of whichever edges the workspace happens to declare today. The
//! live workspace is then checked with the same evaluator.

use std::path::{Path, PathBuf};
use std::process::Command;

use slingshot_development::dependency_direction::{self, DirectionFailure, LocalPackageGraph};

/// Directory holding the metadata documents this test evaluates.
const FIXTURE_DIRECTORY: &str = "crates/slingshot-development/tests/fixtures/dependency-direction";

/// Accepted documents, each of which must produce no diagnostic.
const ACCEPTED_FIXTURES: &[&str] = &[
    "accepted-development-inward-edges.json",
    "accepted-full-graph.json",
    "accepted-product-development-edge-to-test-support.json",
    "accepted-registry-dependencies-only.json",
    "accepted-storage-edge-to-domain.json",
    "accepted-test-support-inward-edges.json",
];

/// Refused documents, each paired with the exact diagnostic it must produce.
const REJECTED_FIXTURES: &[(&str, &str)] = &[
    (
        "rejected-forbidden-product-edge.json",
        "slingshot-domain depends on slingshot-configuration as a normal dependency; \
         slingshot-domain may depend on [], and on slingshot-test-support only as a development dependency",
    ),
    (
        "rejected-product-normal-edge-to-test-support.json",
        "slingshot-daemon depends on slingshot-test-support as a normal dependency;",
    ),
    (
        "rejected-product-build-edge-to-test-support.json",
        "slingshot-storage depends on slingshot-test-support as a build dependency;",
    ),
    (
        "rejected-test-support-edge-to-outer-product.json",
        "slingshot-test-support depends on slingshot-daemon as a normal dependency; \
         slingshot-test-support may depend only on",
    ),
    (
        "rejected-test-support-edge-to-configuration.json",
        "slingshot-test-support depends on slingshot-configuration as a development dependency;",
    ),
    (
        "rejected-dependency-on-development.json",
        "slingshot-daemon depends on slingshot-development as a development dependency;",
    ),
    (
        "rejected-storage-edge-to-agent-protocol.json",
        "slingshot-storage depends on slingshot-agent-protocol as a normal dependency;",
    ),
    (
        "rejected-self-dependency.json",
        "slingshot-daemon depends on slingshot-daemon as a normal dependency;",
    ),
];

/// Document whose local packages form a cycle.
const CYCLIC_FIXTURE: &str = "rejected-cyclic-graph.json";

/// Diagnostic the cyclic document must produce.
const CYCLE_DIAGNOSTIC: &str = "the local packages form the cycle slingshot-configuration -> slingshot-domain -> slingshot-configuration";

/// Registry package every fixture declares, to prove it is ignored.
const REGISTRY_PACKAGE: &str = "serde";

/// Returns the workspace root directory.
fn workspace_root() -> PathBuf {
    slingshot_development::locate_workspace_root(Path::new(env!("CARGO_MANIFEST_DIR")))
        .expect("the development crate lives inside the workspace")
}

/// Reads and parses one metadata document owned by this test.
fn fixture_graph(name: &str) -> LocalPackageGraph {
    let path = workspace_root().join(FIXTURE_DIRECTORY).join(name);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|failure| panic!("{} could not be read: {failure}", path.display()));
    dependency_direction::read_graph(&text)
        .unwrap_or_else(|failure| panic!("{name} is a readable metadata document: {failure}"))
}

/// Returns the resolved metadata of the live workspace.
fn live_graph() -> LocalPackageGraph {
    let mut metadata = Vec::new();
    slingshot_development::emit_workspace_metadata(&workspace_root(), &mut metadata)
        .expect("cargo metadata describes the workspace");
    let text = String::from_utf8(metadata).expect("cargo metadata is text");
    dependency_direction::read_graph(&text).expect("the live metadata reads")
}

#[test]
fn every_accepted_document_and_the_live_workspace_pass() {
    for name in ACCEPTED_FIXTURES {
        assert_eq!(
            dependency_direction::evaluate(&fixture_graph(name)),
            Vec::<String>::new(),
            "{name}"
        );
    }
    assert_eq!(dependency_direction::evaluate(&live_graph()), Vec::<String>::new());
}

#[test]
fn every_refused_document_produces_its_pinned_diagnostic() {
    for (name, expected) in REJECTED_FIXTURES {
        let violations = dependency_direction::evaluate(&fixture_graph(name));
        let matched = violations.iter().filter(|line| line.starts_with(expected)).count();
        assert_eq!(matched, 1, "{name} reports {violations:?}");
        let unrelated: Vec<&String> = violations
            .iter()
            .filter(|line| {
                !line.starts_with(expected) && !line.starts_with("the local packages form")
            })
            .collect();
        assert_eq!(unrelated, Vec::<&String>::new(), "{name} reports an unrelated dependency");
    }
}

#[test]
fn a_cycle_reports_the_complete_local_package_cycle() {
    let violations = dependency_direction::evaluate(&fixture_graph(CYCLIC_FIXTURE));
    assert!(violations.iter().any(|line| line == CYCLE_DIAGNOSTIC), "{violations:?}");
    let repeated = dependency_direction::evaluate(&fixture_graph(CYCLIC_FIXTURE));
    assert_eq!(violations, repeated, "the diagnostics are deterministic");
}

#[test]
fn registry_dependencies_are_outside_the_local_direction_table() {
    let graph = fixture_graph("accepted-registry-dependencies-only.json");
    assert!(!graph.packages.contains(REGISTRY_PACKAGE), "a registry package is not local");
    assert!(graph.edges.is_empty(), "a registry dependency creates no local edge");
    let full = fixture_graph("accepted-full-graph.json");
    assert!(
        full.edges.iter().all(|edge| edge.dependency != REGISTRY_PACKAGE),
        "no local edge names a registry package"
    );
}

#[test]
fn an_unreadable_document_is_refused_before_evaluation() {
    assert!(matches!(
        dependency_direction::read_graph("not metadata"),
        Err(DirectionFailure::Unreadable(_))
    ));
    assert_eq!(
        dependency_direction::read_graph(r#"{"packages":[]}"#),
        Err(DirectionFailure::NoPackages)
    );
}

#[test]
fn the_repository_command_reports_the_live_workspace() {
    let produced = Command::new(slingshot_development::cargo_executable())
        .current_dir(workspace_root())
        .args([
            "run",
            "--locked",
            "--quiet",
            "--package",
            "slingshot-development",
            "--",
            "dependency-direction",
        ])
        .output()
        .expect("the repository command runs");
    assert!(produced.status.success(), "{}", String::from_utf8_lossy(&produced.stderr));
    let rendered = String::from_utf8(produced.stdout).expect("the report is text");
    assert!(rendered.contains("follow the dependency contract"), "{rendered}");

    let refused = Command::new(slingshot_development::cargo_executable())
        .current_dir(workspace_root())
        .args([
            "run",
            "--locked",
            "--quiet",
            "--package",
            "slingshot-development",
            "--",
            "not-a-command",
        ])
        .output()
        .expect("the repository command runs");
    assert!(!refused.status.success(), "an unknown command is refused");
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("unknown repository command"),
        "{}",
        String::from_utf8_lossy(&refused.stderr)
    );
}
