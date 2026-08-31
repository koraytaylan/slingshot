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
const AREA_DOCUMENTS: &[&str] = &[
    "docs/AGENT_PROTOCOL.md",
    "docs/COMMANDS.md",
    "docs/CONFIGURATION.md",
    "docs/DAEMON.md",
    "docs/MODEL_CONTEXT_PROTOCOL.md",
    "docs/WORKFLOWS.md",
];

/// Headings the agent protocol document must carry.
const AGENT_PROTOCOL_HEADINGS: &[&str] = &[
    "# The author agent protocol",
    "## Two protocol versions, and no way out of them",
    "## The head is bounded as it arrives",
    "## Deadlines are per phase, not per request",
    "## What a submission leaves known",
    "## The event stream",
    "## Continuation authority",
    "## Which contract a submission means",
    "## What is not here",
];

/// Transport-contract limits the agent protocol document names.
const NAMED_TRANSPORT_LIMITS: &[&str] = &[
    "maximum_author_response_header_bytes",
    "maximum_author_response_header_count",
    "maximum_author_response_head_bytes",
    "author_connect_timeout_milliseconds",
    "author_tls_timeout_milliseconds",
    "author_request_body_timeout_milliseconds",
    "author_response_header_timeout_milliseconds",
    "maximum_agent_continuation_key_state_bytes",
    "maximum_automatic_retry_attempts",
    "heartbeat_timeout_milliseconds",
];

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

/// What a documented probe writes when nothing owns the target.
const ABSENT_LINE: &str = "daemon-ping: absent";

/// What a documented start writes when it creates the daemon.
const CREATED_LINE: &str = "daemon-start: created";

/// What a documented probe writes when a daemon owns the target.
const SERVING_LINE: &str = "daemon-ping: serving";

/// The member a readiness nonce would appear under if one were published.
const NONCE_MEMBER: &str = "readiness_nonce";

#[test]
fn the_documented_invocations_behave_the_way_they_are_shown() {
    let root = std::env::temp_dir().join(format!("d{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();

    let probed = run_documented(&root, "ping");
    assert!(probed.status.success(), "{}", String::from_utf8_lossy(&probed.stderr));
    let reported = String::from_utf8(probed.stdout).expect("the result is text");
    assert_eq!(reported.trim(), ABSENT_LINE, "{reported}");
    assert!(!root.exists(), "the documented probe creates nothing");

    let started = run_documented(&root, "start");
    assert!(started.status.success(), "{}", String::from_utf8_lossy(&started.stderr));
    let created = String::from_utf8(started.stdout).expect("the result is text");
    assert_eq!(created.trim(), CREATED_LINE, "{created}");

    let running = run_documented(&root, "ping");
    let observed = String::from_utf8(running.stdout).expect("the result is text");
    assert_eq!(observed.trim(), SERVING_LINE, "{observed}");
    assert!(!observed.contains(NONCE_MEMBER), "a documented probe publishes no nonce");

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

/// Security statements the documentation set has to make somewhere.
const SECURITY_STATEMENTS: &[(&str, &str)] = &[
    ("docs/AGENT_PROTOCOL.md", "HTTP/1.1 or HTTP/2"),
    ("docs/AGENT_PROTOCOL.md", "Redirects are disabled everywhere"),
    ("docs/AGENT_PROTOCOL.md", "no partial view of a token"),
    ("docs/AGENT_PROTOCOL.md", "No Java agent"),
    ("docs/COMMANDS.md", "registry's own answer"),
    ("README.md", "Without `--enable-live-author` the leaf is refused"),
    ("README.md", "never fetches, repairs, installs, or"),
    ("CONTRIBUTING.md", "No rule is switched off where it is inconvenient"),
];

/// Prospective language no product document may carry.
const PLANNING_LANGUAGE: &[&str] =
    &["TODO", "FIXME", "will be", "for now", "coming soon", "not yet implemented"];

/// How many pieces a split on the code-span marker produces per span.
const CODE_SPAN_STRIDE: usize = 2;

/// Returns one document with its line breaks flowed into single spaces.
///
/// A sentence a reader meets as one sentence is one sentence, whatever column
/// it happened to wrap at.
fn flowed(document: &str) -> String {
    read_repository_file(document).split_whitespace().collect::<Vec<&str>>().join(" ")
}

/// Returns whether `text` writes `value` out as a number of its own.
///
/// A digit run inside a word - a file name ending in a digest algorithm, say -
/// is part of that word rather than a quantity somebody wrote down.
fn writes_out(text: &str, value: u64) -> bool {
    let spelled = value.to_string();
    let bytes: Vec<char> = text.chars().collect();
    text.match_indices(&spelled).any(|(at, _)| {
        let before = text[..at].chars().next_back();
        let after = bytes.get(at + spelled.len()).copied();
        let bounded = |held: Option<char>| held.is_none_or(|held| !held.is_alphanumeric());
        bounded(before) && bounded(after)
    })
}

/// Returns every product document this repository publishes.
fn every_product_document() -> Vec<&'static str> {
    ROOT_DOCUMENTS.iter().chain(AREA_DOCUMENTS).copied().collect()
}

#[test]
fn the_documentation_set_is_exactly_nine_documents_that_all_exist() {
    let documents = every_product_document();
    let named: BTreeSet<&&str> = documents.iter().collect();
    assert_eq!(named.len(), documents.len(), "a document is listed twice");
    for document in &documents {
        assert!(workspace_root().join(document).is_file(), "{document} does not exist");
    }
    let published = std::fs::read_dir(workspace_root().join("docs"))
        .expect("the documentation directory reads")
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|held| held == "md"))
        .map(|entry| format!("docs/{}", entry.file_name().to_string_lossy()))
        .filter(|path| path != "docs/DOCUMENTATION_REVIEW.md")
        .count();
    assert_eq!(published, AREA_DOCUMENTS.len(), "a document under docs/ is in no contract");
}

#[test]
fn no_product_document_describes_a_plan_rather_than_this_commit() {
    for document in every_product_document() {
        let text = read_repository_file(document);
        for planning in PLANNING_LANGUAGE {
            assert!(!text.contains(planning), "{document} carries planning language: {planning:?}");
        }
    }
}

#[test]
fn the_agent_protocol_document_names_its_bounds_rather_than_restating_them() {
    let document = read_repository_file("docs/AGENT_PROTOCOL.md");
    for heading in AGENT_PROTOCOL_HEADINGS {
        assert!(document.contains(heading), "docs/AGENT_PROTOCOL.md omits {heading:?}");
    }
    let contract =
        slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract::embedded();
    for named in NAMED_TRANSPORT_LIMITS {
        assert!(document.contains(named), "the document does not name {named}");
        let held = contract.limits.get(*named).or_else(|| contract.formulas.get(*named));
        assert!(held.is_some(), "{named} is not a value the transport contract declares");
    }
    for (limit, held) in contract.limits.iter().filter(|(_, held)| **held > u64::from(u8::MAX)) {
        assert!(
            !writes_out(&document, *held),
            "the document writes {limit} out as {held} instead of naming it"
        );
    }
}

#[test]
fn the_five_field_identity_is_described_as_five_fields_and_no_sixth() {
    let document = read_repository_file("docs/AGENT_PROTOCOL.md");
    assert!(document.contains("an identity is five fields"), "the count is stated");
    assert!(
        document.contains("There is no sixth identity member"),
        "and so is the thing that would break it"
    );
    assert!(
        document.contains("*before* the role digest is believed"),
        "and the order the canonical contract is authenticated in"
    );
}

#[test]
fn every_documented_security_statement_is_where_it_belongs() {
    for (document, statement) in SECURITY_STATEMENTS {
        assert!(flowed(document).contains(statement), "{document} does not state {statement:?}");
    }
}

#[test]
fn hermetic_conformance_and_live_evidence_are_never_added_together() {
    let agent = flowed("docs/AGENT_PROTOCOL.md");
    assert!(
        agent.contains("says nothing about any particular installation"),
        "conformance is not evidence about somebody's author"
    );
    let commands = flowed("docs/COMMANDS.md");
    assert!(
        commands.contains("evidence about the author it ran against and about nothing"),
        "and a live run is not evidence about the next one"
    );
    assert!(commands.contains("It is not the release gate"), "and neither replaces the other");
}

#[test]
fn every_documented_command_line_example_parses() {
    let mut examined = 0_usize;
    for document in every_product_document() {
        for line in read_repository_file(document).lines() {
            let Some(rest) = line.trim().strip_prefix("slingshot ") else {
                continue;
            };
            let words: Vec<String> = rest.split_whitespace().map(str::to_owned).collect();
            let normalized = slingshot_command_line::command_line::normalized(&words);
            slingshot_command_line::invocation::parse(&normalized)
                .unwrap_or_else(|refusal| panic!("{document}: `{rest}` does not parse: {refusal}"));
            examined += 1;
        }
    }
    assert!(examined > 0, "the documents show invocations a reader can copy");
}

#[test]
fn every_documented_repository_path_exists() {
    let mut examined = 0_usize;
    for document in every_product_document() {
        let text = read_repository_file(document);
        for candidate in text.split('`').skip(1).step_by(CODE_SPAN_STRIDE) {
            let looks_like_a_path = candidate.contains('/')
                && !candidate.contains(' ')
                && ["policy/", "support/", "scripts/", "schemas/", "compatibility/", "crates/"]
                    .iter()
                    .any(|prefix| candidate.starts_with(prefix));
            if !looks_like_a_path {
                continue;
            }
            assert!(
                workspace_root().join(candidate).exists(),
                "{document} names {candidate}, which does not exist"
            );
            examined += 1;
        }
    }
    assert!(examined > 0, "the documents name the files they describe");
}

#[test]
fn every_documented_link_points_at_something_committed() {
    for document in every_product_document() {
        let text = read_repository_file(document);
        let directory = Path::new(document).parent().unwrap_or(Path::new("")).to_path_buf();
        for piece in text.split("](").skip(1) {
            let Some(target) = piece.split(')').next() else {
                continue;
            };
            if target.starts_with("http") || target.starts_with('#') || target.is_empty() {
                continue;
            }
            let target = target.split('#').next().unwrap_or(target);
            let resolved = workspace_root().join(&directory).join(target);
            assert!(resolved.exists(), "{document} links to {target}, which is not committed");
        }
    }
}

#[test]
fn the_workflow_document_still_carries_what_plan_0008_landed_in_it() {
    let workflows = read_repository_file("docs/WORKFLOWS.md");
    for landed in [
        "## What is pinned",
        "## Which handler does what",
        "## How one command effect is named",
        "## The compatibility gate",
    ] {
        assert!(
            workflows.contains(landed),
            "docs/WORKFLOWS.md lost {landed:?}, which was already there when this plan began"
        );
    }
    let pin = slingshot_development::finite_state_machine_compatibility::
        FiniteStateMachineCompatibilityPin::parse(&read_repository_file(
            slingshot_development::finite_state_machine_compatibility::MANIFEST_PATH,
        ))
        .expect("the compatibility manifest parses");
    assert!(workflows.contains(&pin.commit), "the document names the commit the gate pins");
    assert!(workflows.contains(&pin.repository), "and the repository it pins it in");
}

/// The fence that opens and closes an example.
const FENCE: &str = "```";

/// The member a whole configuration document opens with.
const DOCUMENT_MARKER: &str = "format_version";

/// The member only a profile document carries.
const PROFILE_MARKER: &str = "name = ";

/// The table only a configuration snapshot carries.
const SNAPSHOT_MARKER: &str = "[[sources]]";

/// Returns every fenced example of one language in one document.
fn examples_of(document: &str, language: &str) -> Vec<String> {
    let text = read_repository_file(document);
    let mut examples = Vec::new();
    let mut collecting: Option<Vec<&str>> = None;
    for line in text.lines() {
        match (&mut collecting, line.starts_with(FENCE)) {
            (None, true) if line == format!("{FENCE}{language}") => collecting = Some(Vec::new()),
            (Some(held), true) => {
                examples.push(held.join("\n"));
                collecting = None;
            }
            (Some(held), false) => held.push(line),
            (None, _) => {}
        }
    }
    examples
}

#[test]
fn every_documented_configuration_example_parses_through_the_production_parser() {
    use slingshot_domain::configuration_snapshot::ConfigurationSnapshot;
    use slingshot_domain::profile::{Profile, SelectionDocument};

    let mut whole_documents = 0_usize;
    for document in every_product_document() {
        for example in examples_of(document, "toml") {
            toml::from_str::<toml::Value>(&example)
                .unwrap_or_else(|failure| panic!("{document}: an example is not TOML: {failure}"));
            if !example.contains(DOCUMENT_MARKER) {
                continue;
            }
            whole_documents += 1;
            if example.contains(SNAPSHOT_MARKER) {
                ConfigurationSnapshot::parse(&example).unwrap_or_else(|failure| {
                    panic!("{document}: a snapshot example is refused: {failure:?}")
                });
            } else if example.contains(PROFILE_MARKER) {
                Profile::parse(&example).unwrap_or_else(|failure| {
                    panic!("{document}: a profile example is refused: {failure:?}")
                });
            } else {
                SelectionDocument::parse(&example).unwrap_or_else(|failure| {
                    panic!("{document}: a selection example is refused: {failure:?}")
                });
            }
        }
    }
    assert!(whole_documents > 0, "the documents show configuration a reader can copy");
}

#[test]
fn every_documented_shell_example_names_this_repository_s_own_commands() {
    let mut examined = 0_usize;
    for document in every_product_document() {
        for example in examples_of(document, "sh") {
            let mut continued = false;
            for line in example.lines().map(str::trim) {
                let carries_on = continued;
                continued = line.ends_with('\\');
                if line.is_empty() || line.starts_with('#') || carries_on {
                    continue;
                }
                let named =
                    line.split_whitespace().find(|word| !word.contains('=')).unwrap_or_default();
                let known =
                    named == "slingshot" || named.starts_with("scripts/") || named == "cargo";
                assert!(
                    known,
                    "{document} shows `{line}`, which is not a command this repository has"
                );
                if let Some(path) = named.strip_prefix("scripts/") {
                    assert!(
                        workspace_root().join("scripts").join(path).is_file(),
                        "{document} shows scripts/{path}, which is not committed"
                    );
                }
                examined += 1;
            }
        }
    }
    assert!(examined > 0, "the documents show commands a reader can run");
}
