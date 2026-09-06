//! Resolved-graph guard for the cryptographic backend consolidation.
//!
//! The manifest names direct requests, but Cargo metadata is the graph that a
//! release actually links. This test keeps the selected implementation and
//! every refused implementation in a small independently reviewed fixture.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

const FIXTURE: &str =
    "crates/slingshot-development/tests/fixtures/cryptographic-backend-inventory/graphs.jsonl";

#[derive(Debug, Deserialize)]
struct GraphExpectation {
    package: String,
    version: Option<String>,
    role: String,
}

fn workspace_root() -> PathBuf {
    slingshot_development::locate_workspace_root(Path::new(env!("CARGO_MANIFEST_DIR")))
        .expect("the development crate lives inside the workspace")
}

fn expectations() -> Vec<GraphExpectation> {
    std::fs::read_to_string(workspace_root().join(FIXTURE))
        .expect("the backend graph fixture is readable")
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("each graph fixture row is JSON"))
        .collect()
}

fn metadata() -> serde_json::Value {
    let output = Command::new(slingshot_development::cargo_executable())
        .current_dir(workspace_root())
        .args(["metadata", "--locked", "--offline", "--format-version", "1"])
        .output()
        .expect("cargo metadata starts");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    serde_json::from_slice(&output.stdout).expect("cargo metadata is JSON")
}

#[test]
fn the_resolved_graph_has_one_selected_backend_and_no_refused_implementation() {
    let metadata = metadata();
    let packages: BTreeSet<(&str, &str)> = metadata["packages"]
        .as_array()
        .expect("metadata packages")
        .iter()
        .filter_map(|package| Some((package["name"].as_str()?, package["version"].as_str()?)))
        .collect();
    let rows = expectations();
    let selected: Vec<_> = rows.iter().filter(|row| row.role == "selected").collect();
    assert_eq!(selected.len(), 1, "the fixture names one selected backend");
    for row in selected {
        assert!(
            packages.contains(&(row.package.as_str(), row.version.as_deref().unwrap_or_default()))
        );
    }
    for row in rows.iter().filter(|row| row.role == "forbidden") {
        assert!(
            !packages.iter().any(|(name, _)| *name == row.package),
            "{} remains in the graph",
            row.package
        );
    }
}
