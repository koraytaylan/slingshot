//! The release tag and the workspace version must name one release.

use std::path::{Path, PathBuf};
use std::process::Command;

fn workspace_root() -> PathBuf {
    slingshot_development::locate_workspace_root(Path::new(env!("CARGO_MANIFEST_DIR")))
        .expect("the development crate is inside the workspace")
}

fn run(reference: Option<&str>, tag: Option<&str>) -> std::process::Output {
    let mut command = Command::new(workspace_root().join("scripts/verify_release_version"));
    command.current_dir(workspace_root());
    command.env_remove("SLINGSHOT_RELEASE_REFERENCE_TYPE");
    command.env_remove("SLINGSHOT_RELEASE_TAG");
    if let Some(reference) = reference {
        command.env("SLINGSHOT_RELEASE_REFERENCE_TYPE", reference);
    }
    if let Some(tag) = tag {
        command.env("SLINGSHOT_RELEASE_TAG", tag);
    }
    command.output().expect("the release version checker runs")
}

#[test]
fn the_declared_release_tag_is_accepted() {
    let output = run(Some("tag"), Some("v0.1.0"));
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(String::from_utf8_lossy(&output.stdout).contains("both name 0.1.0"));
}

#[test]
fn a_tag_for_another_version_is_refused_before_release_work() {
    let output = run(Some("tag"), Some("v0.2.0"));
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("declares 0.1.0"));
}

#[test]
fn the_release_prefix_is_part_of_the_contract() {
    let output = run(Some("tag"), Some("0.1.0"));
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("does not begin with v"));
}

#[test]
fn a_manual_run_reports_the_declared_version_without_inventing_a_tag() {
    let output = run(Some("branch"), None);
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("publishes 0.1.0"));
}

#[test]
fn a_tagged_run_without_the_tag_name_is_refused() {
    let output = run(Some("tag"), None);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("named none"));
}
