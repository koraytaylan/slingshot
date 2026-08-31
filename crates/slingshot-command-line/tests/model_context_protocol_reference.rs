//! The protocol reference, generated from the metadata it describes.
//!
//! The same discipline as the command reference: every table is rendered from
//! what the server actually reads, and the committed document is compared
//! against that rendering. A reference a model host trusts and a build that has
//! moved on are worse than no reference, because the host has no way to tell.

use std::path::PathBuf;

use slingshot_command_line::model_context_protocol::current_stateless_revision::EVERY_ERROR;
use slingshot_command_line::model_context_protocol::resource_catalog::{
    ARTIFACT_TEMPLATE, MAINTENANCE_TEMPLATE, OPERATION_TEMPLATE,
};
use slingshot_command_line::model_context_protocol::standard_stream_transport::SUPPORTED_REVISIONS;
use slingshot_command_line::model_context_protocol::tool_catalog::{
    KeyPresence, Provenance, derive,
};

/// Where the reference lives.
const REFERENCE: &str = "../../docs/MODEL_CONTEXT_PROTOCOL.md";

/// The variable that arms a rewrite of the generated sections.
const REVIEW_VARIABLE: &str = "SLINGSHOT_REVIEW_PROTOCOL_REFERENCE";

/// The command a reviewer runs to rewrite them.
const REVIEW_COMMAND: &str = "SLINGSHOT_REVIEW_PROTOCOL_REFERENCE=1 \
     cargo test -p slingshot-command-line --test model_context_protocol_reference";

/// Words that describe a plan rather than what this build does.
const PLANNING_WORDS: &[&str] = &["TODO", "FIXME", "will be", "for now", "coming soon", "not yet"];

/// Headings the reference carries.
const HEADINGS: &[&str] = &[
    "# The Model Context Protocol server",
    "## Starting it",
    "## Revisions",
    "## Tools",
    "## Resources",
    "## Answers",
    "## Cancelling",
    "## What is not here",
];

/// Claims the reference has to make, because a host that assumes otherwise
/// loses work or repeats it.
const EVERY_CLAIM: &[&str] = &[
    "Nothing remote is ever asked to stop",
    "the same bytes a command line writes",
    "invented, once",
    "every line there is one protocol message",
];

/// One generated section, and what renders it.
type Section = (&'static str, fn() -> String);

/// Every section this suite renders from metadata.
const EVERY_SECTION: &[Section] = &[
    ("revisions", revisions),
    ("tools", tools),
    ("resource-templates", resource_templates),
    ("errors", errors),
];

/// Returns the reference as it is committed.
fn reference() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(REFERENCE);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|failure| panic!("{} could not be read: {failure}", path.display()))
}

/// Returns the revisions this build serves, in the order it prefers them.
fn revisions() -> String {
    SUPPORTED_REVISIONS
        .iter()
        .enumerate()
        .map(|(position, revision)| {
            let preference = if position == 0 { "preferred" } else { "offered" };
            format!("- `{revision}` ({preference})")
        })
        .collect::<Vec<String>>()
        .join("\n")
}

/// Returns the table of tools this server offers.
fn tools() -> String {
    let mut lines = vec![
        "| Tool | Read-only | Destructive | Same call twice | Operation key |".to_owned(),
        "|---|---|---|---|---|".to_owned(),
    ];
    for tool in derive(&Provenance::recomputed()).expect("this build's provenance agrees") {
        let key = match tool.operation_key {
            KeyPresence::Required => "required",
            KeyPresence::Optional => "optional",
            KeyPresence::Absent => "none",
        };
        lines.push(format!(
            "| `{}` | {} | {} | {} | {key} |",
            tool.name, tool.read_only_hint, tool.destructive_hint, tool.idempotent_hint
        ));
    }
    lines.join("\n")
}

/// Returns the addresses this server publishes.
fn resource_templates() -> String {
    [OPERATION_TEMPLATE, ARTIFACT_TEMPLATE, MAINTENANCE_TEMPLATE]
        .iter()
        .map(|template| format!("- `{}`", template.split_whitespace().collect::<String>()))
        .collect::<Vec<String>>()
        .join("\n")
}

/// Returns the errors a client may receive.
fn errors() -> String {
    let mut lines = vec!["| Code | When |".to_owned(), "|---|---|".to_owned()];
    for code in EVERY_ERROR {
        lines.push(format!("| `{code}` | {} |", meaning_of(*code)));
    }
    lines.join("\n")
}

/// Returns what one error code means, in the words the reference uses.
fn meaning_of(code: i64) -> &'static str {
    match code {
        -32_700 => "The line could not be read as a message.",
        -32_600 => "The line was read and is not a request.",
        -32_601 => "This server offers no such method.",
        -32_602 => "The arguments cannot be used.",
        -32_603 => "This server failed to answer.",
        _ => "This build serves neither of the revisions the request names.",
    }
}

/// Returns the document with every generated section rendered from metadata.
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
fn every_generated_section_matches_the_metadata_it_describes() {
    let committed = reference();
    let rendered = with_generated_sections(&committed);
    if std::env::var(REVIEW_VARIABLE).is_ok() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(REFERENCE);
        std::fs::write(&path, rendered).expect("the reference is written");
        return;
    }
    assert_eq!(rendered, committed, "the reference drifted; rewrite it with `{REVIEW_COMMAND}`");
}

#[test]
fn the_reference_carries_its_headings_and_describes_the_present() {
    let document = reference();
    for heading in HEADINGS {
        assert!(document.contains(heading), "the reference is missing {heading:?}");
    }
    for planning in PLANNING_WORDS {
        assert!(
            !document.contains(planning),
            "the reference carries planning language: {planning:?}"
        );
    }
}

#[test]
fn the_reference_makes_the_claims_a_host_has_to_be_able_to_rely_on() {
    let flowed = reference().split_whitespace().collect::<Vec<&str>>().join(" ");
    for claim in EVERY_CLAIM {
        assert!(flowed.contains(claim), "the reference does not say: {claim:?}");
    }
}

#[test]
fn every_tool_and_every_error_is_named() {
    let document = reference();
    for tool in derive(&Provenance::recomputed()).expect("this build's provenance agrees") {
        assert!(
            document.contains(&format!("`{}`", tool.name)),
            "the reference omits {}",
            tool.name
        );
    }
    for code in EVERY_ERROR {
        assert!(document.contains(&format!("`{code}`")), "the reference omits {code}");
    }
    for revision in SUPPORTED_REVISIONS {
        assert!(document.contains(&format!("`{revision}`")), "the reference omits {revision}");
    }
}

#[test]
fn every_link_the_reference_makes_reaches_something() {
    let document = reference();
    let beside = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(REFERENCE);
    let root = beside.parent().expect("the reference lives in a directory").to_path_buf();
    let mut scanning = document.as_str();
    let mut linked = 0_usize;
    while let Some(position) = scanning.find("](") {
        let after = &scanning[position + 2..];
        let end = after.find(')').expect("every link closes");
        let target = &after[..end];
        scanning = &after[end..];
        if target.starts_with("http") || target.starts_with('#') {
            continue;
        }
        linked += 1;
        assert!(root.join(target).exists(), "{target} does not exist");
    }
    assert!(linked > 0, "a reference that links to nothing is a reference to nothing");
}
