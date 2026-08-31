//! Assertions for the root documents.
//!
//! Every claim the documents make about this commit is checked against the
//! commit: the files they link to exist, the crates they name are the crates
//! the workspace has, the platform rows they show are the rows the manifest
//! declares, and the invocations they show behave the way they are shown.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use slingshot_development::supported_platform_matrix::{self, SupportedPlatformMatrix};

/// The three root documents.
const ROOT_DOCUMENTS: &[&str] = &["README.md", "CONTRIBUTING.md", "ARCHITECTURE.md"];

/// Documents below the repository root that describe one product area.
const AREA_DOCUMENTS: &[&str] = &["docs/CONFIGURATION.md", "docs/DAEMON.md"];

/// Headings the daemon document must carry.
const DAEMON_HEADINGS: &[&str] = &[
    "# The daemon",
    "## One target, one daemon, one owner",
    "## Two roots",
    "## Reaching readiness",
    "## What execution does in this build",
    "## Facts an operation can be in",
    "## Waiting, listing, and reading",
    "## Resuming and maintaining",
    "## Stopping",
    "## Diagnostics",
    "## What is not here",
];

/// Headings the README must carry.
const README_HEADINGS: &[&str] = &[
    "# Slingshot",
    "## What this commit does",
    "## Crates",
    "## Supported targets",
    "## Limits",
    "## Checking a change",
];

/// Headings the contributing guide must carry.
const CONTRIBUTING_HEADINGS: &[&str] = &[
    "## Claims come with the assertions that prove them",
    "## Unchecked code",
    "## Names and values",
    "## Size and shape",
    "## Documentation",
    "## Dependency direction",
    "## Footprints",
    "## Workflows",
    "## The gate",
];

/// Headings the architecture document must carry.
const ARCHITECTURE_HEADINGS: &[&str] = &[
    "## The crate graph",
    "## One target, one daemon",
    "## Starting and stopping",
    "## The local request path",
    "## Platforms",
    "## Limits",
    "## How the rules are enforced",
    "## What is not here",
];

/// Repository paths the documents refer to and that must exist.
const REFERENCED_PATHS: &[&str] = &[
    "support/platforms.toml",
    "support/foundation-contract.toml",
    "support/platform-runtime-evidence.schema.json",
    "compatibility/rustsec-advisory-database.toml",
    "policy/abbreviated-identifiers.txt",
    "policy/external-interface-identifiers.toml",
    "policy/documentation-rules.toml",
    "policy/source-policy.toml",
    "policy/workspace-capabilities.toml",
    "scripts/quality",
];

/// Claims no document may make while the evidence for them does not exist.
const REFUSED_CLAIMS: &[&str] = &[
    "release ready",
    "release-ready",
    "production ready",
    "all platforms verified",
    "fully verified",
    "every row verified",
];

/// Profile the documented invocation names.
const DOCUMENTED_PROFILE: &str = "local";

/// Environment the documented invocation names.
const DOCUMENTED_ENVIRONMENT: &str = "author";

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
    supported_platform_matrix::parse_matrix(&read_repository_file("support/platforms.toml"))
        .expect("the committed matrix is valid")
}

/// Returns the package names the workspace declares.
fn workspace_packages() -> BTreeSet<String> {
    let mut metadata = Vec::new();
    slingshot_development::emit_workspace_metadata(&workspace_root(), &mut metadata)
        .expect("cargo metadata describes the workspace");
    let document: serde_json::Value =
        serde_json::from_slice(&metadata).expect("cargo metadata is well-formed");
    document["packages"]
        .as_array()
        .expect("cargo metadata lists packages")
        .iter()
        .filter_map(|package| package["name"].as_str().map(str::to_owned))
        .collect()
}

/// Runs the product executable inside a temporary runtime root.
fn run_documented(root: &Path, action: &str) -> std::process::Output {
    Command::new(slingshot_development::cargo_executable())
        .current_dir(workspace_root())
        .args(["run", "--locked", "--quiet", "--package", "slingshot-command-line", "--"])
        .args(["--profile", DOCUMENTED_PROFILE, "--environment", DOCUMENTED_ENVIRONMENT])
        .arg("--runtime-root")
        .arg(root)
        .args(["daemon", action])
        .output()
        .expect("the documented invocation runs")
}

#[test]
fn every_document_carries_its_headings_and_links_to_files_that_exist() {
    for (document, headings) in [
        ("README.md", README_HEADINGS),
        ("CONTRIBUTING.md", CONTRIBUTING_HEADINGS),
        ("ARCHITECTURE.md", ARCHITECTURE_HEADINGS),
    ] {
        let text = read_repository_file(document);
        for heading in headings {
            assert!(text.contains(heading), "{document} omits {heading}");
        }
    }
    let combined: String = ROOT_DOCUMENTS.iter().map(|name| read_repository_file(name)).collect();
    for referenced in REFERENCED_PATHS {
        assert!(combined.contains(referenced), "no document refers to {referenced}");
        assert!(workspace_root().join(referenced).exists(), "{referenced} does not exist");
    }
    for document in ROOT_DOCUMENTS {
        assert!(workspace_root().join(document).is_file(), "{document} does not exist");
    }
    let readme = read_repository_file("README.md");
    for linked in ["CONTRIBUTING.md", "ARCHITECTURE.md"] {
        assert!(readme.contains(&format!("]({linked})")), "the README does not link to {linked}");
    }
}

#[test]
fn every_area_document_carries_its_headings_and_makes_no_claim_it_cannot_prove() {
    for relative in AREA_DOCUMENTS {
        let document = read_repository_file(relative);
        assert!(!document.trim().is_empty(), "{relative} is empty");
        for claim in REFUSED_CLAIMS {
            assert!(
                !document.to_lowercase().contains(claim),
                "{relative} claims {claim:?}, and no evidence for that exists"
            );
        }
    }

    let daemon = read_repository_file("docs/DAEMON.md");
    for heading in DAEMON_HEADINGS {
        assert!(daemon.contains(heading), "docs/DAEMON.md is missing {heading:?}");
    }
    assert!(
        daemon.contains("## What is not here"),
        "a document that never says what is absent reads as a document about a finished thing"
    );
}

#[test]
fn the_daemon_document_describes_the_present_rather_than_a_plan() {
    let daemon = read_repository_file("docs/DAEMON.md");
    for planning in ["TODO", "FIXME", "will be", "for now", "coming soon", "not yet implemented"] {
        assert!(
            !daemon.contains(planning),
            "docs/DAEMON.md carries planning language: {planning:?}"
        );
    }
    assert!(
        daemon.contains("installs the author-backed operation executor"),
        "and names the executor a product build actually runs work through"
    );
    assert!(
        daemon.contains("installed rather than chosen"),
        "and says why no deployment can end up running the one that runs nothing"
    );
    assert!(
        daemon.contains("neither an ending nor"),
        "and states the distinction the whole recovery vocabulary exists to keep"
    );
}

#[test]
fn the_documented_crate_map_is_the_workspace() {
    let readme = read_repository_file("README.md");
    let architecture = read_repository_file("ARCHITECTURE.md");
    for package in workspace_packages() {
        assert!(readme.contains(&package), "the crate map omits {package}");
        assert!(architecture.contains(&package), "the dependency table omits {package}");
    }
    assert!(readme.contains("`slingshot` executable"), "the product executable is named");
    assert!(architecture.contains("slingshot-development` is the repository-command executable"));
}

#[test]
fn every_documented_target_row_is_a_row_the_manifest_declares() {
    let readme = read_repository_file("README.md");
    let matrix = committed_matrix();
    for row in &matrix.target {
        let executable = format!("{}{}", row.executable_stem, row.executable_suffix);
        let documented = format!(
            "| `{}` | `{executable}` | `{}` | `{}` |",
            row.triple, row.archive_profile, row.native_smoke_mode
        );
        assert!(readme.contains(&documented), "the target table omits {documented}");
    }
    let rows = readme.matches("x86_64-unknown-linux-gnu").count();
    assert!(rows > 0, "the target table names the rows it declares");
    assert!(readme.contains("untrusted_current_native_observation"));
    assert!(readme.contains("makes no aggregate claim across rows"));
}

#[test]
fn no_document_claims_evidence_that_does_not_exist() {
    for document in ROOT_DOCUMENTS {
        let text = read_repository_file(document).to_lowercase();
        for claim in REFUSED_CLAIMS {
            assert!(!text.contains(claim), "{document} claims {claim}");
        }
    }
    let readme = read_repository_file("README.md");
    assert!(readme.contains("Every package is unpublished"));
    assert!(readme.contains("Experience Manager behavior exists here yet"));
    let architecture = read_repository_file("ARCHITECTURE.md");
    assert!(architecture.contains("## What is not here"));
}

#[test]
fn the_documented_invocations_behave_the_way_they_are_shown() {
    let root = std::env::temp_dir().join(format!("d{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();

    let probed = run_documented(&root, "ping");
    assert!(probed.status.success(), "{}", String::from_utf8_lossy(&probed.stderr));
    let reported = String::from_utf8(probed.stdout).expect("the result is text");
    assert!(reported.contains("\"running\":false"), "{reported}");
    assert!(!root.exists(), "the documented probe creates nothing");

    let started = run_documented(&root, "start");
    assert!(started.status.success(), "{}", String::from_utf8_lossy(&started.stderr));
    let created: serde_json::Value =
        serde_json::from_str(String::from_utf8(started.stdout).expect("text").trim())
            .expect("the result reads");
    assert_eq!(created["profile"].as_str(), Some(DOCUMENTED_PROFILE));
    assert_eq!(created["environment"].as_str(), Some(DOCUMENTED_ENVIRONMENT));

    let running = run_documented(&root, "ping");
    let observed: serde_json::Value =
        serde_json::from_str(String::from_utf8(running.stdout).expect("text").trim())
            .expect("the result reads");
    assert_eq!(observed["running"].as_bool(), Some(true));
    assert_eq!(observed["readiness_nonce"], created["readiness_nonce"]);

    stop_documented_daemon(&root);
    std::fs::remove_dir_all(&root).ok();
}

/// Stops the daemon the documented invocation created.
///
/// The stop is written over a blocking connection so this assertion needs no
/// asynchronous runtime of its own: the framing and the envelope are pure, and
/// the daemon acknowledges before it shuts down.
#[cfg(unix)]
fn stop_documented_daemon(root: &Path) {
    use std::io::{Read, Write};

    let contract = slingshot_local_protocol::foundation_contract::FoundationContract::embedded();
    let namespace = slingshot_daemon::runtime_namespace::RuntimeNamespace::name(
        &contract,
        root,
        DOCUMENTED_PROFILE,
        DOCUMENTED_ENVIRONMENT,
    )
    .expect("the documented target names a namespace");
    let Some(record) =
        slingshot_daemon::platform_runtime::readiness::read(root, namespace.digest())
            .expect("the record is readable")
    else {
        return;
    };
    let address = slingshot_daemon::platform_runtime::endpoint::endpoint_address(
        &contract,
        root,
        namespace.digest(),
    )
    .expect("the endpoint is named");
    let slingshot_daemon::platform_runtime::endpoint::EndpointAddress::UnixDomainSocket(path) =
        &address;
    let request = slingshot_local_protocol::envelope::ControlRequest {
        control_version: contract.control.version,
        request_identifier: "documentation-cleanup".to_owned(),
        method: slingshot_local_protocol::ping::STOP_METHOD.to_owned(),
        arguments: serde_json::json!({ "readiness_nonce": record.readiness_nonce }),
    };
    let payload = serde_json::to_vec(&request).expect("the request renders");
    let frame = slingshot_local_protocol::framing::render(&contract.framing, &payload)
        .expect("the request frames");
    let mut stream = std::os::unix::net::UnixStream::connect(path).expect("the client connects");
    stream.write_all(&frame).expect("the request is written");
    let mut acknowledgement = Vec::new();
    stream.read_to_end(&mut acknowledgement).expect("the acknowledgement arrives");
    assert!(!acknowledgement.is_empty(), "the daemon acknowledged its cooperative stop");
}

/// Stops the daemon the documented invocation created.
#[cfg(not(unix))]
fn stop_documented_daemon(root: &Path) {
    let _unreached_on_this_row = root;
}
