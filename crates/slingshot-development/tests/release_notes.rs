//! Release-note assembly must ask the authenticated changelog tool for the
//! correct history range and write both durable documents.

use std::path::{Path, PathBuf};
use std::process::Command;

fn workspace_root() -> PathBuf {
    slingshot_development::locate_workspace_root(Path::new(env!("CARGO_MANIFEST_DIR")))
        .expect("the development crate is inside the workspace")
}

fn executable(path: &Path, body: &str) {
    std::fs::write(path, body).expect("the test helper is written");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(path).expect("the helper exists").permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(path, permissions).expect("the helper is executable");
    }
}

fn run(reference: Option<&str>) -> (tempfile::TempDir, std::process::Output) {
    let root = tempfile::tempdir().expect("a fixture root");
    let tools = root.path().join("tools");
    std::fs::create_dir(&tools).expect("the tool directory");
    let fake_tool = root.path().join("fake-git-cliff");
    executable(
        &fake_tool,
        r#"#!/bin/sh
if [ "$1" = "--version" ]; then printf 'git-cliff 2.14.1\n'; exit 0; fi
printf '%s\n' "$*" >> "$FAKE_GIT_CLIFF_LOG"
output=''
while [ "$#" -gt 0 ]; do
  if [ "$1" = "--output" ]; then output=$2; shift 2; continue; fi
  shift
done
case "$FAKE_GIT_CLIFF_LOG_MODE" in
  latest) printf 'tagged feature\n' > "$output" ;;
  unreleased) printf 'unreleased feature\n' > "$output" ;;
  *) printf 'complete history\n' > "$output" ;;
esac
"#,
    );
    let fake_curl = tools.join("curl");
    executable(
        &fake_curl,
        "#!/bin/sh\nwhile [ \"$#\" -gt 0 ]; do if [ \"$1\" = \"--output\" ]; then : > \"$2\"; shift 2; else shift; fi; done\n",
    );
    let fake_sha = tools.join("sha256sum");
    executable(&fake_sha, "#!/bin/sh\nexit 0\n");
    let fake_tar = tools.join("tar");
    executable(
        &fake_tar,
        "#!/bin/sh\ndestination=''\nwhile [ \"$#\" -gt 0 ]; do if [ \"$1\" = \"-C\" ]; then destination=$2; shift 2; else shift; fi; done\nmkdir -p \"$destination/git-cliff-2.14.1\"\ncp \"$FAKE_GIT_CLIFF\" \"$destination/git-cliff-2.14.1/git-cliff\"\nchmod 755 \"$destination/git-cliff-2.14.1/git-cliff\"\n",
    );
    let log = root.path().join("git-cliff.log");
    let destination = root.path().join("output");
    let mut path = tools.into_os_string();
    path.push(":");
    path.push(std::env::var_os("PATH").expect("PATH is present"));
    let mut command = Command::new(workspace_root().join("scripts/build_release_notes"));
    command.current_dir(workspace_root());
    command.arg(&destination);
    command.env("PATH", path);
    command.env("FAKE_GIT_CLIFF", &fake_tool);
    command.env("FAKE_GIT_CLIFF_LOG", &log);
    command.env("FAKE_GIT_CLIFF_LOG_MODE", reference.map_or("unreleased", |_| "latest"));
    command.env_remove("SLINGSHOT_RELEASE_REFERENCE_TYPE");
    if let Some(reference) = reference {
        command.env("SLINGSHOT_RELEASE_REFERENCE_TYPE", reference);
    }
    let output = command.output().expect("the notes builder runs");
    (root, output)
}

#[test]
fn a_tagged_run_asks_for_latest_history_and_writes_notes() {
    let (root, output) = run(Some("tag"));
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let notes = std::fs::read_to_string(root.path().join("output/RELEASE_NOTES.md"))
        .expect("release notes are written");
    assert!(notes.contains("tagged feature"));
    let invocation = std::fs::read_to_string(root.path().join("git-cliff.log"))
        .expect("the tool invocation is recorded");
    assert!(invocation.contains("--latest"));
    assert!(!invocation.contains("--unreleased"));
    assert!(root.path().join("output/CHANGELOG.md").is_file());
}

#[test]
fn a_manual_run_asks_for_unreleased_history() {
    let (root, output) = run(None);
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let notes = std::fs::read_to_string(root.path().join("output/RELEASE_NOTES.md"))
        .expect("release notes are written");
    assert!(notes.contains("unreleased feature"));
    let invocation = std::fs::read_to_string(root.path().join("git-cliff.log"))
        .expect("the tool invocation is recorded");
    assert!(invocation.contains("--unreleased"));
    assert!(!invocation.contains("--latest"));
}
