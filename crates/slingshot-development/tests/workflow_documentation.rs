//! The workflow documentation, against the contract it describes.
//!
//! Three of its sections are rendered from the manifest and the key contract
//! rather than written beside them, because those are exactly the values a
//! reader would copy and exactly the values that move. The prose is
//! hand-written and checked for the claims it has to make and the language it
//! must not use.
//!
//! Every command the document shows is one this repository actually has. A
//! document that shows a command nothing runs stops being true without anybody
//! noticing.

use std::path::PathBuf;

use slingshot_development::finite_state_machine_compatibility::{
    FiniteStateMachineCompatibilityPin, MANIFEST_PATH,
};
use slingshot_development::finite_state_machine_handler_validation::{
    EVERY_SUFFIX, KEY_PREFIX, KEY_PREIMAGE_FORMAT, MOST_INPUT_UTF8_BYTES, MOST_KEY_BYTES,
};

/// Where the document lives.
const DOCUMENT: &str = "docs/WORKFLOWS.md";

/// Where the command inventory lives.
const COMMAND_FIXTURE: &str = "tests/fixtures/workflow-documentation-commands.txt";

/// The variable that arms a rewrite of the generated sections.
const REVIEW_VARIABLE: &str = "SLINGSHOT_REVIEW_WORKFLOW_DOCUMENTATION";

/// The command a reviewer runs to rewrite them.
const REVIEW_COMMAND: &str = "SLINGSHOT_REVIEW_WORKFLOW_DOCUMENTATION=1 \
     cargo test -p slingshot-development --test workflow_documentation";

/// How many directories separate this crate from the workspace root.
const CRATE_DEPTH: usize = 2;

/// Words that describe a plan rather than what this repository does.
const PLANNING_WORDS: &[&str] = &["TODO", "FIXME", "will be", "for now", "coming soon", "not yet"];

/// Headings the document carries.
const HEADINGS: &[&str] = &[
    "# Workflows",
    "## What is pinned",
    "## How the processes fit together",
    "## Which handler does what",
    "## How one command effect is named",
    "## What a workflow journals",
    "## What entitles a workflow to undo something",
    "## Retries and restarts",
    "## Examples",
    "## What is not here",
];

/// Claims the document has to make.
const EVERY_CLAIM: &[&str] = &[
    "establishes no hosted-provider",
    "the same intended occurrence always derives the same key",
    "A maintenance control carries no key at all",
    "ends the call and not the work",
    "Two gates, in order, and both required",
];

/// Returns the workspace root.
fn workspace_root() -> PathBuf {
    let mut root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for _ in 0..CRATE_DEPTH {
        root = root.parent().expect("the crate is inside the workspace").to_path_buf();
    }
    root
}

/// Reads one file from the workspace.
fn read_repository_file(relative: &str) -> String {
    let path = workspace_root().join(relative);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|failure| panic!("{} could not be read: {failure}", path.display()))
}

/// Returns the committed pin.
fn pin() -> FiniteStateMachineCompatibilityPin {
    FiniteStateMachineCompatibilityPin::parse(&read_repository_file(MANIFEST_PATH))
        .expect("the committed manifest parses")
}

/// One generated section, and what renders it.
type Section = (&'static str, fn() -> String);

/// Every section this suite renders.
const EVERY_SECTION: &[Section] =
    &[("pin", pin_section), ("operation-key", key_section), ("examples", examples_section)];

/// Returns the table of what is pinned.
fn pin_section() -> String {
    let held = pin();
    [
        "| What | Value |".to_owned(),
        "|---|---|".to_owned(),
        format!("| Repository | `{}` |", held.repository),
        format!("| Commit | `{}` |", held.commit),
        format!("| Protocol revision | `{}` |", held.model_context_protocol_revision),
        format!("| Handler format | `{}` |", held.handler_format),
        format!(
            "| Daemon runtime contract | `{}` at `{}` |",
            held.daemon_runtime_contract_format, held.daemon_runtime_contract_sha256
        ),
        format!(
            "| Author-agent transport contract | `{}` at `{}` |",
            held.author_agent_transport_contract_format,
            held.author_agent_transport_contract_sha256
        ),
    ]
    .join("\n")
}

/// Returns the description of how a key is derived.
fn key_section() -> String {
    let suffixes = EVERY_SUFFIX
        .iter()
        .map(
            |suffix| {
                if suffix.is_empty() { "the empty one".to_owned() } else { format!("`{suffix}`") }
            },
        )
        .collect::<Vec<String>>()
        .join(" and ");
    [
        format!(
            "The preimage is one object declaring `{KEY_PREIMAGE_FORMAT}`, with no whitespace, \
             its members in byte order, and the occurrence in minimal base ten. Its digest, \
             prefixed with `{KEY_PREFIX}`, is the key."
        ),
        String::new(),
        format!(
            "- Each input is a nonempty valid-UTF-8 string of at most {MOST_INPUT_UTF8_BYTES} \
             bytes, carrying no control code point."
        ),
        format!("- The only suffixes are {suffixes}."),
        format!("- A key is at most {MOST_KEY_BYTES} bytes."),
    ]
    .join("\n")
}

/// Returns the examples this repository commits.
fn examples_section() -> String {
    let directory = workspace_root().join("examples/finite-state-machine");
    let mut named: Vec<String> = std::fs::read_dir(&directory)
        .expect("the examples are committed")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    named.sort();
    named
        .iter()
        .map(|name| format!("- [`{name}`](../examples/finite-state-machine/{name})"))
        .collect::<Vec<String>>()
        .join("\n")
}

/// Returns the document with every generated section rendered.
fn with_generated_sections(document: &str) -> String {
    let mut written = document.to_owned();
    for (name, render) in EVERY_SECTION {
        let open = format!("<!-- generated: {name} -->");
        let close = format!("<!-- end generated: {name} -->");
        let start = written.find(&open).unwrap_or_else(|| panic!("no {name} section"));
        let end = written.find(&close).unwrap_or_else(|| panic!("{name} is not closed"));
        let body = format!("{open}\n\n{}\n\n", render());
        written = format!("{}{body}{}", &written[..start], &written[end..]);
    }
    written
}

#[test]
fn every_generated_section_matches_the_contract_it_describes() {
    let committed = read_repository_file(DOCUMENT);
    let rendered = with_generated_sections(&committed);
    if std::env::var(REVIEW_VARIABLE).is_ok() {
        std::fs::write(workspace_root().join(DOCUMENT), rendered).expect("it is written");
        return;
    }
    assert_eq!(rendered, committed, "the document drifted; rewrite it with `{REVIEW_COMMAND}`");
}

#[test]
fn the_document_carries_its_headings_and_describes_the_present() {
    let document = read_repository_file(DOCUMENT);
    for heading in HEADINGS {
        assert!(document.contains(heading), "the document is missing {heading:?}");
    }
    for planning in PLANNING_WORDS {
        assert!(
            !document.contains(planning),
            "the document carries planning language: {planning:?}"
        );
    }
    let flowed = document.split_whitespace().collect::<Vec<&str>>().join(" ");
    for claim in EVERY_CLAIM {
        assert!(flowed.contains(claim), "the document does not say: {claim:?}");
    }
}

#[test]
fn every_command_the_document_stands_on_is_one_this_repository_has() {
    let inventory =
        std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(COMMAND_FIXTURE))
            .expect("the inventory is committed");
    let mut declared = 0;
    for line in inventory.lines().filter(|line| !line.starts_with('#') && !line.trim().is_empty()) {
        let (command, why) = line.split_once('|').expect("every row names a command and why");
        declared += 1;
        assert!(!why.trim().is_empty(), "{command} says what it checks");
        let named = command.rsplit("--test ").next().expect("every command names a suite");
        let suite =
            workspace_root().join("crates/slingshot-development/tests").join(format!("{named}.rs"));
        assert!(suite.is_file(), "{command} names a suite that does not exist");
    }
    assert!(declared > 0, "a document standing on nothing stands on nothing");
}

#[test]
fn the_document_names_no_developers_machine() {
    let document = read_repository_file(DOCUMENT);
    for particular in ["/home/", "/Users/", "C:\\\\Users"] {
        assert!(!document.contains(particular), "the document names {particular}");
    }
}

#[test]
fn every_committed_example_is_listed_and_every_listed_example_exists() {
    let document = read_repository_file(DOCUMENT);
    let directory = workspace_root().join("examples/finite-state-machine");
    for entry in std::fs::read_dir(&directory).expect("the examples are committed") {
        let name = entry.expect("the entry is readable").file_name().to_string_lossy().into_owned();
        assert!(document.contains(&format!("`{name}`")), "the document omits {name}");
    }
}
