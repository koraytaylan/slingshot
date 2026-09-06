//! Publishing must authenticate the complete evidence set before uploading.

use std::path::{Path, PathBuf};
use std::process::Command;

fn workspace_root() -> PathBuf {
    slingshot_development::locate_workspace_root(Path::new(env!("CARGO_MANIFEST_DIR")))
        .expect("the development crate is inside the workspace")
}

fn executable(path: &Path, body: &str) {
    std::fs::write(path, body).expect("the helper is written");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(path).expect("the helper exists").permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(path, permissions).expect("the helper is executable");
    }
}

#[test]
fn an_invalid_later_attestation_stops_before_any_release_upload() {
    let root = tempfile::tempdir().expect("a fixture root");
    let tools = root.path().join("tools");
    std::fs::create_dir(&tools).expect("the tools directory");
    let log = root.path().join("provider.log");
    executable(
        &tools.join("git"),
        "#!/bin/sh\ncase \"$1\" in\n  status) exit 0 ;;\n  rev-parse) case \"$2\" in *^{tree}) printf 'tree\\n' ;; *) printf 'commit\\n' ;; esac ;;\n  ls-remote) printf 'commit\\n' ;;\n  *) exit 0 ;;\nesac\n",
    );
    executable(&tools.join("cargo"), "#!/bin/sh\nexit 0\n");
    executable(
        &tools.join("gh"),
        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$FAKE_PROVIDER_LOG\"\nif [ \"$1\" = \"attestation\" ] && printf '%s' \"$3\" | grep -q 'bad'; then exit 1; fi\nexit 0\n",
    );
    let mut path = tools.into_os_string();
    path.push(":");
    path.push(std::env::var_os("PATH").expect("PATH is present"));

    let evidence = root.path().join("evidence");
    std::fs::create_dir_all(evidence.join("release-notes")).expect("notes directory");
    std::fs::create_dir_all(evidence.join("release-acceptance")).expect("acceptance directory");
    std::fs::write(evidence.join("release-notes/RELEASE_NOTES.md"), "notes\n").unwrap();
    std::fs::write(evidence.join("release-acceptance/acceptance.json"), "manifest\n").unwrap();
    for (row, archive) in [("aarch64-apple-darwin", "good.tar.gz"), ("x86_64-unknown-linux-gnu", "bad.tar.gz")] {
        let directory = evidence.join(format!("release-{row}"));
        std::fs::create_dir(&directory).unwrap();
        std::fs::write(directory.join(archive), b"archive").unwrap();
        std::fs::write(directory.join("attestation.jsonl"), "attestation\n").unwrap();
        std::fs::write(directory.join("evidence.toml"), "cache-sha256 = \"cache\"\n").unwrap();
    }

    let output = Command::new(workspace_root().join("scripts/publish_release"))
        .current_dir(workspace_root())
        .args(["--tag", "v0.1.0", "--evidence"])
        .arg(&evidence)
        .env("PATH", path)
        .env("FAKE_PROVIDER_LOG", &log)
        .output()
        .expect("the publisher runs");
    assert!(!output.status.success(), "invalid attestation must refuse publication");
    let provider_calls = std::fs::read_to_string(&log).unwrap_or_else(|failure| {
        panic!("the verifier did not call the provider: {failure}; stdout={}; stderr={}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr))
    });
    assert!(provider_calls.contains("attestation verify"));
    assert!(!provider_calls.contains("release upload"));
    assert!(!provider_calls.contains("release create"));
}
