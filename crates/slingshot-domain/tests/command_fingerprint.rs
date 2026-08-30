//! The proof a repeated identifier describes the same work.
//!
//! Every expected digest below is computed outside this crate from the four
//! values that go into the preimage. What the vectors are really pinning is
//! which changes move the fingerprint and which do not: a genuine
//! same-principal rotation leaves the upstream target and revision alone and
//! the fingerprint holds, while a changed principal arrives as a changed target
//! and a changed metascope or trust policy arrives as a changed revision. This
//! module never decides which of those happened - it is handed two opaque
//! values and it hashes them - and the test is written to make that visible
//! rather than to imply otherwise.

use serde_json::Value;
use slingshot_domain::command_fingerprint::{
    CommandFingerprint, DIGEST_CHARACTERS, FIELD_SEPARATOR, FINGERPRINT_ENCODING_VERSION,
    FingerprintFailure, FingerprintInput, RepeatedIdentifier, classify_repeat,
};

/// Command vectors this test reads.
const COMMANDS: &str = include_str!("fixtures/command_fingerprint/commands.jsonl");

/// Drift vectors this test reads.
const DRIFT: &str = include_str!("fixtures/command_fingerprint/drift.jsonl");

/// Ordering vectors this test reads.
const ORDERINGS: &str = include_str!("fixtures/command_fingerprint/orderings.jsonl");

/// Rendering vectors this test reads.
const RENDERINGS: &str = include_str!("fixtures/command_fingerprint/renderings.jsonl");

/// Reads one row's string member.
fn text<'row>(row: &'row Value, member: &str) -> &'row str {
    row[member].as_str().unwrap_or_else(|| panic!("{member} is a string in {row}"))
}

/// Returns every row of one fixture.
fn rows(fixture: &str) -> Vec<Value> {
    fixture
        .lines()
        .map(|line| serde_json::from_str(line).expect("every fixture line is one object"))
        .collect()
}

/// Returns the input one fixture row describes.
fn input_of(row: &Value) -> FingerprintInput {
    FingerprintInput {
        author_target_identity_digest: text(row, "author_target_identity_digest").to_owned(),
        canonical_command: text(row, "canonical_command").to_owned(),
        command_semantic_contract_version: text(row, "command_semantic_contract_version")
            .to_owned(),
        command_wire_name: text(row, "command_wire_name").to_owned(),
        selected_environment_revision: text(row, "selected_environment_revision").to_owned(),
    }
}

#[test]
fn every_command_vector_derives_its_own_recorded_digest() {
    let vectors = rows(COMMANDS);
    assert_eq!(vectors.len(), 12, "one for each command this catalog declares");
    let mut derived = std::collections::BTreeSet::new();
    for row in &vectors {
        let fingerprint = CommandFingerprint::derive(&input_of(row)).expect("a legal input");
        assert_eq!(fingerprint.as_text(), text(row, "fingerprint"), "{}", text(row, "note"));
        derived.insert(fingerprint.as_text().to_owned());
    }
    assert_eq!(derived.len(), vectors.len(), "no two commands share a fingerprint");
}

#[test]
fn every_drift_vector_moves_the_fingerprint_exactly_when_the_fixture_says() {
    let vectors = rows(DRIFT);
    assert!(vectors.len() >= 9, "rotation, principal, revision, command, and version");
    for row in &vectors {
        let fingerprint = CommandFingerprint::derive(&input_of(row)).expect("a legal input");
        assert_eq!(fingerprint.as_text(), text(row, "fingerprint"), "{}", text(row, "note"));
        let changed = fingerprint.as_text() != text(row, "base_fingerprint");
        assert_eq!(changed, row["changes"].as_bool().expect("a verdict"), "{}", text(row, "note"));
    }
}

#[test]
fn a_genuine_rotation_keeps_a_retry_a_retry() {
    let vectors = rows(DRIFT);
    let rotated = vectors
        .iter()
        .find(|row| text(row, "note").contains("rotation"))
        .expect("a rotation vector");
    let unchanged = vectors
        .iter()
        .find(|row| text(row, "note") == "nothing changed")
        .expect("a baseline vector");
    assert_eq!(
        CommandFingerprint::derive(&input_of(rotated)).expect("legal"),
        CommandFingerprint::derive(&input_of(unchanged)).expect("legal"),
        "the upstream target and revision did not move, so neither does this"
    );
}

#[test]
fn a_repeat_is_a_retry_only_when_the_revision_agrees_too() {
    let vectors = rows(COMMANDS);
    let first = CommandFingerprint::derive(&input_of(&vectors[0])).expect("legal");
    let second = CommandFingerprint::derive(&input_of(&vectors[1])).expect("legal");
    let revision = text(&vectors[0], "selected_environment_revision");
    let other_revision = "9".repeat(DIGEST_CHARACTERS);

    assert_eq!(
        classify_repeat(&first, revision, &first, revision),
        RepeatedIdentifier::Retry,
        "the same work described the same way"
    );
    assert_eq!(
        classify_repeat(&first, revision, &second, revision),
        RepeatedIdentifier::Conflict,
        "a different command wearing the same name"
    );
    assert_eq!(
        classify_repeat(&first, revision, &first, &other_revision),
        RepeatedIdentifier::Conflict,
        "the revision is compared on its own, because a digest agreeing with a digest \
         is not a daemon agreeing with what it started from"
    );
}

#[test]
fn the_preimage_cannot_be_produced_by_running_two_fields_together() {
    let mut input = FingerprintInput {
        author_target_identity_digest: "a".repeat(DIGEST_CHARACTERS),
        canonical_command: r#"{"root_path":"/content"}"#.to_owned(),
        command_semantic_contract_version: "1.0.0".to_owned(),
        command_wire_name: "query_paths".to_owned(),
        selected_environment_revision: "c".repeat(DIGEST_CHARACTERS),
    };
    assert!(CommandFingerprint::derive(&input).is_ok());
    input.canonical_command.push(char::from(FIELD_SEPARATOR));
    assert_eq!(
        CommandFingerprint::derive(&input),
        Err(FingerprintFailure::SeparatorInPart),
        "a part carrying the separator could make two inputs into one preimage"
    );
    assert_eq!(FIELD_SEPARATOR, 0);
    assert_eq!(FINGERPRINT_ENCODING_VERSION, "slingshot.command-fingerprint/1");
}

#[test]
fn the_encoding_version_is_part_of_what_is_hashed() {
    let input = FingerprintInput {
        author_target_identity_digest: "a".repeat(DIGEST_CHARACTERS),
        canonical_command: r#"{"root_path":"/content"}"#.to_owned(),
        command_semantic_contract_version: "1.0.0".to_owned(),
        command_wire_name: "query_paths".to_owned(),
        selected_environment_revision: "c".repeat(DIGEST_CHARACTERS),
    };
    let fingerprint = CommandFingerprint::derive(&input).expect("legal");
    let without_version = {
        use sha2::Digest as _;
        let mut hasher = sha2::Sha256::new();
        for (position, part) in [
            input.author_target_identity_digest.as_str(),
            input.selected_environment_revision.as_str(),
            input.command_semantic_contract_version.as_str(),
            input.command_wire_name.as_str(),
            input.canonical_command.as_str(),
        ]
        .iter()
        .enumerate()
        {
            if position > 0 {
                hasher.update([FIELD_SEPARATOR]);
            }
            hasher.update(part.as_bytes());
        }
        hasher.finalize().iter().map(|octet| format!("{octet:02x}")).collect::<String>()
    };
    assert_ne!(
        fingerprint.as_text(),
        without_version,
        "a later encoding cannot produce a value this one produced"
    );
}

#[test]
fn two_spellings_of_one_command_are_two_fingerprints_unless_they_are_one_spelling() {
    for row in &rows(ORDERINGS) {
        let build = |command: &str| {
            CommandFingerprint::derive(&FingerprintInput {
                author_target_identity_digest: "a".repeat(DIGEST_CHARACTERS),
                canonical_command: command.to_owned(),
                command_semantic_contract_version: "1.0.0".to_owned(),
                command_wire_name: "load_content_as_json".to_owned(),
                selected_environment_revision: "c".repeat(DIGEST_CHARACTERS),
            })
            .expect("legal")
        };
        assert_eq!(
            build(text(row, "left")) == build(text(row, "right")),
            row["same"].as_bool().expect("a verdict"),
            "{}: this hashes the canonical bytes it is handed, and canonicalizing them \
             is the caller's job",
            text(row, "note")
        );
    }
}

#[test]
fn every_rendering_vector_is_read_the_way_the_fixture_says() {
    for row in &rows(RENDERINGS) {
        let outcome = CommandFingerprint::parse(text(row, "spelling"));
        assert_eq!(
            outcome.is_ok(),
            row["accepted"].as_bool().expect("a verdict"),
            "{}",
            text(row, "note")
        );
        if outcome.is_err() {
            assert_eq!(outcome, Err(FingerprintFailure::NotCanonical));
        }
    }
}

#[test]
fn nothing_about_a_request_that_is_not_the_work_goes_in() {
    let code: String = include_str!("../src/command_fingerprint.rs")
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<&str>>()
        .join("\n");
    for absent in [
        "request_identifier",
        "timestamp",
        "cursor",
        "publisher",
        "password",
        "user_name",
        "process_identifier",
    ] {
        assert!(
            !code.contains(absent),
            "a fingerprint that moved with {absent} would make \
                                         every retry a conflict"
        );
    }
}
