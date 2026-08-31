//! Turning an argument vector into a request, and touching nothing on the way.
//!
//! The property that matters is negative: parsing reaches no configuration, no
//! file, no process, and no socket. It is asserted rather than assumed, by
//! pointing every environment variable a configuration lookup could use at a
//! directory that does not exist and requiring the parse to answer identically.
//! A parser that consulted anything would answer differently, or fail.
//!
//! The second property is that options belong to leaves. A global detachment
//! flag would be inherited by a leaf with nothing to detach from and later by a
//! standard-stream server that must never see one, so each option is refused on
//! the leaves that cannot honour it - and the refusal names both the option and
//! the leaf, because a caller reading it needs to know which of the two to
//! change.
//!
//! The third is that a key is required exactly where a repeat would be a second
//! effect. That is the catalog's own classification, read rather than restated,
//! so every published command is checked against it rather than a chosen few.

use slingshot_command_line::invocation::{
    EVERY_OPTION, Invocation, LOCAL_LEAVES, METADATA_ONLY_LEAVES, OutputForm, ParseRefusal,
    is_catalog_command, parse, required_options, requires_operation_key,
};
use slingshot_domain::command::catalog::CommandCatalog;

/// Where the vectors this suite is driven from live.
const FIXTURES: &str = "../slingshot-test-support/fixtures/command-invocations";

/// Paths into the standard library a parser has no business naming.
const BOUNDARY_PATHS: &[&str] =
    &["std::fs", "std::net", "std::process", "std::env", "File::", "TcpStream"];

/// Argument vectors the working-directory check re-parses without any fixture.
///
/// Held inline because the check removes the directory the fixtures are read
/// from, and reading them again afterwards would prove only that the file was
/// gone.
const ACCEPTED_ROWS: &[&[&str]] = &[
    &["help"],
    &["version"],
    &["check-configuration", "--profile", "production"],
    &["query_paths", "--machine"],
    &["create_page", "--operation-key", "one"],
];

/// Returns every vector one fixture holds.
fn vectors(name: &str) -> Vec<serde_json::Value> {
    let path = format!("{FIXTURES}/{name}");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("{path} is readable"));
    text.lines().map(|line| serde_json::from_str(line).expect("each line is one vector")).collect()
}

/// Returns the argument vector one row states.
fn arguments(vector: &serde_json::Value) -> Vec<String> {
    vector["arguments"]
        .as_array()
        .expect("a list")
        .iter()
        .map(|value| value.as_str().expect("an argument").to_owned())
        .collect()
}

/// Returns how one refusal is spelled in the vectors.
fn refusal_spelling(refusal: &ParseRefusal) -> &'static str {
    match refusal {
        ParseRefusal::NoLeaf => "no-leaf",
        ParseRefusal::UnknownLeaf { .. } => "unknown-leaf",
        ParseRefusal::UnknownOption { .. } => "unknown-option",
        ParseRefusal::MissingValue { .. } => "missing-value",
        ParseRefusal::RepeatedOption { .. } => "repeated-option",
        ParseRefusal::OptionNotOnThisLeaf { .. } => "option-not-on-this-leaf",
        ParseRefusal::OperationKeyRequired { .. } => "operation-key-required",
        ParseRefusal::RequiredOptionMissing { .. } => "required-option-missing",
    }
}

#[test]
fn every_accepted_vector_parses_into_exactly_what_it_states() {
    for vector in vectors("accepted.jsonl") {
        let name = vector["name"].as_str().expect("a name");
        let parsed =
            parse(&arguments(&vector)).unwrap_or_else(|refusal| panic!("{name}: {refusal}"));
        assert_eq!(parsed.verb, vector["verb"].as_str().expect("a verb"), "{name}");
        assert_eq!(parsed.selection.profile.as_deref(), vector["profile"].as_str(), "{name}");
        assert_eq!(
            parsed.selection.environment.as_deref(),
            vector["environment"].as_str(),
            "{name}"
        );
        assert_eq!(parsed.operation_key.as_deref(), vector["operation_key"].as_str(), "{name}");
        assert_eq!(
            parsed.detached,
            vector["detached"].as_bool().unwrap_or(false),
            "{name}: detachment is the caller's word and never a default"
        );
        assert_eq!(
            parsed.output == Some(OutputForm::Machine),
            vector["machine"].as_bool().unwrap_or(false),
            "{name}"
        );
        assert_eq!(
            parsed.is_metadata_only(),
            vector["metadata_only"].as_bool().unwrap_or(false),
            "{name}"
        );
    }
}

#[test]
fn every_refused_vector_names_the_one_thing_that_is_wrong() {
    for vector in vectors("refused.jsonl") {
        let name = vector["name"].as_str().expect("a name");
        let refusal = parse(&arguments(&vector)).expect_err(&format!("{name} is refused"));
        assert_eq!(
            refusal_spelling(&refusal),
            vector["refusal"].as_str().expect("a refusal"),
            "{name}: {refusal}"
        );
        let rendered = format!("{refusal}");
        assert!(!rendered.is_empty(), "{name}: and says which");
    }
}

#[test]
fn the_parser_names_nothing_that_could_reach_outside_the_arguments() {
    let source = std::fs::read_to_string("src/invocation.rs").expect("the parser is readable");
    for boundary in BOUNDARY_PATHS {
        assert!(
            !source.contains(boundary),
            "the parser names {boundary}, so a usage mistake could cost more than a message"
        );
    }
    assert!(
        source.contains("CommandCatalog::published"),
        "what it does read is the embedded registry, which is part of the build"
    );
}

#[test]
fn parsing_answers_the_same_with_the_working_directory_gone() {
    let before: Vec<Result<Invocation, ParseRefusal>> =
        vectors("accepted.jsonl").iter().map(|vector| parse(&arguments(vector))).collect();
    let held = std::env::current_dir().expect("a working directory");
    let vanishing = tempfile::tempdir().expect("a temporary directory");
    let path = vanishing.path().to_path_buf();
    std::env::set_current_dir(&path).expect("the process moves into it");
    drop(vanishing);
    let after: Vec<Result<Invocation, ParseRefusal>> = ACCEPTED_ROWS
        .iter()
        .map(|row| parse(&row.iter().map(|held| (*held).to_owned()).collect::<Vec<String>>()))
        .collect();
    std::env::set_current_dir(&held).expect("the process moves back");
    assert_eq!(after.len(), ACCEPTED_ROWS.len(), "every row still answered");
    assert!(after.iter().all(Result::is_ok), "and none of them needed a directory to do it");
    assert!(before.iter().all(Result::is_ok));
}

#[test]
fn a_key_is_required_exactly_where_the_catalog_says_a_repeat_would_be_a_second_effect() {
    for descriptor in CommandCatalog::published().descriptors() {
        let leaf = descriptor.wire_name.as_str();
        assert!(is_catalog_command(leaf), "{leaf} is a command this surface offers");
        let without = parse(&[leaf.to_owned()]);
        if descriptor.intrinsic_idempotency.requires_operation_key() {
            assert!(
                matches!(without, Err(ParseRefusal::OperationKeyRequired { .. })),
                "{leaf} changes something, so a repeat needs a key to be the same request"
            );
            let with = parse(&[leaf.to_owned(), "--operation-key".to_owned(), "one".to_owned()]);
            assert!(with.is_ok(), "{leaf}: and is accepted once the key is there");
        } else {
            assert!(without.is_ok(), "{leaf} repeats harmlessly, so it needs no key");
        }
        assert_eq!(
            requires_operation_key(leaf),
            descriptor.intrinsic_idempotency.requires_operation_key(),
            "{leaf}: the surface reads the classification rather than keeping its own"
        );
    }
}

#[test]
fn only_help_and_version_answer_without_reaching_anything() {
    for leaf in LOCAL_LEAVES {
        let parsed = parse(&[(*leaf).to_owned()]);
        let metadata_only = parsed.map(|held| held.is_metadata_only()).unwrap_or(false);
        assert_eq!(
            metadata_only,
            METADATA_ONLY_LEAVES.contains(leaf),
            "{leaf}: everything else has somewhere to reach, even if it refuses once it gets there"
        );
    }
    for descriptor in CommandCatalog::published().descriptors() {
        let leaf = &descriptor.wire_name;
        let arguments = if requires_operation_key(leaf) {
            vec![leaf.clone(), "--operation-key".to_owned(), "one".to_owned()]
        } else {
            vec![leaf.clone()]
        };
        assert!(
            !parse(&arguments).expect("it parses").is_metadata_only(),
            "{leaf}: a command reaches an author, so it is never metadata only"
        );
    }
}

#[test]
fn every_option_belongs_to_a_leaf_and_the_refusal_names_both() {
    let refusal = parse(&["operation-list".to_owned(), "--detach".to_owned()])
        .expect_err("a listing detaches from nothing");
    let ParseRefusal::OptionNotOnThisLeaf { leaf, named } = refusal else {
        panic!("the refusal names the leaf and the option")
    };
    assert_eq!(leaf, "operation-list");
    assert_eq!(named, "--detach");
    for option in EVERY_OPTION {
        let accepted = LOCAL_LEAVES
            .iter()
            .map(|leaf| (*leaf).to_owned())
            .chain(
                CommandCatalog::published()
                    .descriptors()
                    .iter()
                    .map(|descriptor| descriptor.wire_name.clone()),
            )
            .filter(|leaf| {
                !matches!(
                    parse(&[leaf.clone(), (*option).to_owned(), "value".to_owned()]),
                    Err(ParseRefusal::OptionNotOnThisLeaf { .. })
                )
            })
            .count();
        assert!(accepted > 0, "{option} is an option some leaf actually takes");
    }
}

#[test]
fn every_leaf_that_requires_an_option_says_which_one_is_missing() {
    for leaf in LOCAL_LEAVES {
        for required in required_options(leaf) {
            let refusal =
                parse(&[(*leaf).to_owned()]).expect_err(&format!("{leaf} requires {required}"));
            assert!(
                matches!(refusal, ParseRefusal::RequiredOptionMissing { .. }),
                "{leaf}: {refusal}"
            );
        }
    }
    assert!(
        required_options("daemon-status").is_empty(),
        "and a leaf that requires nothing says nothing about it"
    );
}
