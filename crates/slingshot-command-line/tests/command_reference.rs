//! The command reference, generated from the metadata it describes.
//!
//! A reference written by hand drifts: an option is added, a failure category
//! is registered, an exit is classified, and the document keeps describing what
//! the build used to do. So every table here is rendered from the same metadata
//! the executable reads, and the committed document is compared against that
//! rendering byte for byte. A table that disagrees is a failing test rather than
//! a paragraph somebody has to notice.
//!
//! Prose stays hand-written, between the markers. What a reader needs to be
//! told - that an interrupt cancels nothing, that a publication is the success,
//! that a pre-receipt interruption promises nothing about durability - is a
//! judgement, and generating it would produce sentences nobody chose.
//!
//! Every example is parsed. An example that does not parse is worse than no
//! example, because a reader trusts it.

use std::collections::BTreeSet;
use std::path::PathBuf;

use slingshot_command_line::application::{Service, service_for};
use slingshot_command_line::command_line::normalized;
use slingshot_command_line::exit_classification::{
    AGENT_REJECTION, EVERY_EXIT, INDETERMINATE, INTERRUPTED, LOCAL_FAILURE, REMOTE_FAILURE,
    SUCCESS, UNAVAILABLE, USAGE,
};
use slingshot_command_line::human_renderer::{
    POST_RECEIPT_TEMPLATE, PRE_RECEIPT_TEMPLATE, TRANSFER_TEMPLATE,
};
use slingshot_command_line::invocation::{LOCAL_LEAVES, leaves_taking, parse};
use slingshot_command_line::machine_outcome_envelope::MachineOutcomeEnvelope;
use slingshot_domain::command::catalog::CommandCatalog;

/// Where the reference lives.
const REFERENCE: &str = "../../docs/COMMANDS.md";

/// The variable that arms a rewrite of the generated sections.
const REVIEW_VARIABLE: &str = "SLINGSHOT_REVIEW_COMMAND_REFERENCE";

/// The command a reviewer runs to rewrite the generated sections.
const REVIEW_COMMAND: &str = "SLINGSHOT_REVIEW_COMMAND_REFERENCE=1 \
     cargo test -p slingshot-command-line --test command_reference";

/// Words that describe a plan rather than what this build does.
const PLANNING_WORDS: &[&str] = &["TODO", "FIXME", "will be", "for now", "coming soon", "not yet"];

/// Headings the reference carries.
const HEADINGS: &[&str] = &[
    "# Commands",
    "## Reading a command line",
    "## Commands this build offers",
    "## Commands the registry publishes",
    "## What a command can answer",
    "## How a command can fail",
    "## Exits",
    "## Interruption",
    "## Examples",
    "## What is not here",
];

/// Returns the reference as it is committed.
fn reference() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(REFERENCE);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|failure| panic!("{} could not be read: {failure}", path.display()))
}

/// Writes the reference back with its generated sections rendered again.
fn rewrite(rendered: &str) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(REFERENCE);
    std::fs::write(&path, rendered).expect("the reference is written");
}

/// Returns what one generated section is named in the document.
fn opening(name: &str) -> String {
    format!("<!-- generated: {name} -->")
}

/// Returns what closes one generated section.
fn closing(name: &str) -> String {
    format!("<!-- end generated: {name} -->")
}

/// Returns the document with every generated section rendered from metadata.
fn with_generated_sections(document: &str) -> String {
    let mut written = document.to_owned();
    for (name, render) in EVERY_SECTION {
        let open = opening(name);
        let close = closing(name);
        let start =
            written.find(&open).unwrap_or_else(|| panic!("the reference has no {name} section"));
        let end = written.find(&close).unwrap_or_else(|| panic!("{name} is not closed"));
        let body = format!("{open}\n\n{}\n\n", render());
        written = format!("{}{body}{}", &written[..start], &written[end..]);
    }
    written
}

/// One generated section, and what renders it.
type Section = (&'static str, fn() -> String);

/// Every section this suite renders from metadata.
const EVERY_SECTION: &[Section] = &[
    ("local-leaves", local_leaves),
    ("registry-commands", registry_commands),
    ("failure-categories", failure_categories),
    ("outcome-tags", outcome_tags),
    ("exits", exits),
    ("interruption-templates", interruption_templates),
];

/// Returns the table of leaves this build offers.
fn local_leaves() -> String {
    let mut lines = vec!["| Leaf | Options it takes |".to_owned(), "|---|---|".to_owned()];
    for leaf in LOCAL_LEAVES {
        let mut taken: Vec<String> = Vec::new();
        for option in slingshot_command_line::invocation::EVERY_OPTION {
            if leaves_taking(option).iter().any(|named| named == leaf) {
                taken.push(format!("`{option}`"));
            }
        }
        let held = if taken.is_empty() { "none".to_owned() } else { taken.join(", ") };
        lines.push(format!("| `{leaf}` | {held} |"));
    }
    lines.join("\n")
}

/// Returns the table of commands the registry publishes.
fn registry_commands() -> String {
    let mut lines = vec![
        "| Command | What it does | Access | Operation key | Result bound |".to_owned(),
        "|---|---|---|---|---|".to_owned(),
    ];
    for descriptor in CommandCatalog::published().descriptors() {
        let key = if descriptor.intrinsic_idempotency.requires_operation_key() {
            "required"
        } else {
            "refused"
        };
        lines.push(format!(
            "| `{}` | {} | {:?} | {key} | {} bytes |",
            descriptor.wire_name,
            descriptor.title,
            descriptor.access,
            descriptor.maximum_result_bytes
        ));
    }
    lines.join("\n")
}

/// Returns the table of failures each command may report.
fn failure_categories() -> String {
    let mut lines =
        vec!["| Command | Failure categories it registers |".to_owned(), "|---|---|".to_owned()];
    for descriptor in CommandCatalog::published().descriptors() {
        let categories: Vec<String> =
            descriptor.failure_categories.iter().map(|held| format!("`{held}`")).collect();
        lines.push(format!("| `{}` | {} |", descriptor.wire_name, categories.join(", ")));
    }
    lines.join("\n")
}

/// Returns the list of tags a machine-readable answer may carry.
fn outcome_tags() -> String {
    MachineOutcomeEnvelope::EVERY_TAG
        .iter()
        .map(|tag| format!("- `{tag}`"))
        .collect::<Vec<String>>()
        .join("\n")
}

/// What each exit means, in the words the reference uses.
const EVERY_EXIT_MEANING: &[(i32, &str)] = &[
    (SUCCESS, "The command finished and its answer is on standard output."),
    (USAGE, "The invocation is wrong. Nothing was reached and nothing was changed."),
    (AGENT_REJECTION, "The author refused the work. It provably did not run."),
    (REMOTE_FAILURE, "The work ran and failed."),
    (
        INDETERMINATE,
        "Nobody can say whether the work ran. Running it again risks running it twice.",
    ),
    (UNAVAILABLE, "What the command needed was not there, or would not agree with this build."),
    (LOCAL_FAILURE, "Something on this machine failed. Nothing remote is claimed."),
    (INTERRUPTED, "Somebody asked the run to stop. Nothing remote was asked to stop."),
];

/// Returns the table of exits this build produces.
fn exits() -> String {
    let mut lines = vec!["| Exit | What it means |".to_owned(), "|---|---|".to_owned()];
    for (exit, meaning) in EVERY_EXIT_MEANING {
        lines.push(format!("| `{exit}` | {meaning} |"));
    }
    lines.join("\n")
}

/// Returns the three lines an interrupted run may print.
fn interruption_templates() -> String {
    [
        format!("- Before the daemon answered: `{PRE_RECEIPT_TEMPLATE}`"),
        format!("- After it answered: `{POST_RECEIPT_TEMPLATE}`"),
        format!("- While fetching: `{TRANSFER_TEMPLATE}`"),
    ]
    .join("\n")
}

// ---------------------------------------------------------------- assertions

#[test]
fn every_generated_section_matches_the_metadata_it_describes() {
    let committed = reference();
    let rendered = with_generated_sections(&committed);
    if std::env::var(REVIEW_VARIABLE).is_ok() {
        rewrite(&rendered);
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

/// Returns every command line one fenced shell block holds.
fn examples(document: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut scanning = document;
    while let Some(position) = scanning.find("```sh") {
        let after = &scanning[position + "```sh".len()..];
        let end = after.find("```").expect("every fence closes");
        for line in after[..end].lines() {
            let line = line.trim();
            if let Some(command) = line.strip_prefix("slingshot ") {
                found.push(command.to_owned());
            }
        }
        scanning = &after[end..];
    }
    found
}

#[test]
fn every_example_parses_and_reaches_one_service() {
    let found = examples(&reference());
    assert!(
        found.len() >= LEAST_EXAMPLES,
        "a reference with {} examples explains little",
        found.len()
    );
    let mut reached: BTreeSet<String> = BTreeSet::new();
    for example in &found {
        let words: Vec<String> = example.split_whitespace().map(str::to_owned).collect();
        let invocation = parse(&normalized(&words))
            .unwrap_or_else(|refusal| panic!("`slingshot {example}` does not parse: {refusal}"));
        let service = service_for(&invocation)
            .unwrap_or_else(|refusal| panic!("`slingshot {example}` reaches nothing: {refusal}"));
        reached.insert(format!("{service:?}"));
    }
    for service in EVERY_SERVICE {
        assert!(reached.contains(*service), "no example reaches {service}");
    }
}

/// How many examples a reference has to carry to be one.
const LEAST_EXAMPLES: usize = 8;

/// Every service an example is expected to reach.
const EVERY_SERVICE: &[&str] = &[
    "Metadata",
    "ConfigurationCheck",
    "DaemonLifecycle",
    "OperationSubmission",
    "OperationObservation",
    "OperationMaintenance",
];

#[test]
fn one_example_parses_to_exactly_the_request_the_command_tests_pin() {
    let words: Vec<String> = PINNED_EXAMPLE.split_whitespace().map(str::to_owned).collect();
    let invocation = parse(&normalized(&words)).expect("the pinned example parses");
    assert_eq!(invocation.verb, "load_content_as_json");
    assert_eq!(invocation.selection.profile.as_deref(), Some("local"));
    assert_eq!(invocation.selection.environment.as_deref(), Some("author"));
    assert_eq!(invocation.arguments.get("--path").map(String::as_str), Some("/content/site/en"));
    assert_eq!(invocation.operation_key.as_deref(), Some("one-read"));
    assert_eq!(service_for(&invocation), Ok(Service::OperationSubmission));
}

/// The example whose parsed request this suite pins exactly.
const PINNED_EXAMPLE: &str = "--profile local --environment author load_content_as_json \
     --path /content/site/en --operation-key one-read";

#[test]
fn the_reference_says_what_an_interrupt_does_and_does_not_do() {
    let document = reference();
    for template in [PRE_RECEIPT_TEMPLATE, POST_RECEIPT_TEMPLATE, TRANSFER_TEMPLATE] {
        assert!(document.contains(template), "the reference paraphrases {template:?}");
    }

    let flowed = document.split_whitespace().collect::<Vec<&str>>().join(" ");
    for claim in EVERY_CLAIM {
        assert!(flowed.contains(claim), "the reference does not say: {claim:?}");
    }
}

/// Claims the reference has to make, because a reader who assumes otherwise
/// loses work or repeats it.
const EVERY_CLAIM: &[&str] = &[
    "An interrupt stops this process and nothing else",
    "publication is the success",
    "A pre-receipt interruption claims nothing about durability",
    "Standard output carries the answer",
    "exactly one envelope",
];

#[test]
fn every_exit_this_build_classifies_is_described_once() {
    let described: Vec<i32> = EVERY_EXIT_MEANING.iter().map(|(exit, _)| *exit).collect();
    for exit in EVERY_EXIT {
        assert!(described.contains(exit), "{exit} is classified and undescribed");
    }
    assert_eq!(
        described.len(),
        EVERY_EXIT.len(),
        "the reference describes an exit nothing produces"
    );
    let document = reference();
    for (exit, meaning) in EVERY_EXIT_MEANING {
        assert!(document.contains(&format!("`{exit}`")), "the reference omits exit {exit}");
        assert!(document.contains(meaning), "the reference omits what {exit} means");
    }
}

#[test]
fn every_registry_command_and_every_leaf_is_named() {
    let document = reference();
    for leaf in LOCAL_LEAVES {
        assert!(document.contains(&format!("`{leaf}`")), "the reference omits {leaf}");
    }
    for descriptor in CommandCatalog::published().descriptors() {
        assert!(
            document.contains(&format!("`{}`", descriptor.wire_name)),
            "the reference omits {}",
            descriptor.wire_name
        );
        for category in &descriptor.failure_categories {
            assert!(
                document.contains(&format!("`{category}`")),
                "the reference omits the {category} failure of {}",
                descriptor.wire_name
            );
        }
    }
    for tag in MachineOutcomeEnvelope::EVERY_TAG {
        assert!(document.contains(&format!("`{tag}`")), "the reference omits the {tag} answer");
    }
}
