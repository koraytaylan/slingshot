//! Operator evidence that stays bounded and carries no secret out with it.
//!
//! The redaction vectors are the point of the suite. Eight of them carry
//! something a file must never end up holding; three carry nothing to hide and
//! must survive intact, because a sink that redacted everything would be as
//! useless as one that redacted nothing.

use serde_json::Value;
use slingshot_daemon::diagnostics::{
    DiagnosticBounds, DiagnosticFailure, DiagnosticSink, REDACTED_PATH, REDACTION, redact,
};

/// Redaction vectors this test reads.
const VECTORS: &str = include_str!("fixtures/diagnostics/redaction.jsonl");

/// Bytes one record may occupy, from the runtime contract.
const RECORD_BYTES: usize = 4096;

/// Bytes one field may occupy, from the runtime contract.
const FIELD_BYTES: usize = 1024;

/// Bytes one file may reach, from the runtime contract.
const FILE_BYTES: u64 = 1_048_576;

/// Files kept beside the active one, from the runtime contract.
const RETAINED_FILES: usize = 4;

/// Bytes every diagnostic file may occupy together, from the runtime contract.
const TOTAL_BYTES: u64 = 4_194_304;

/// Bytes one file may reach under the small fixture bounds.
const SMALL_FILE_BYTES: u64 = 256;

/// The divisor a fixture line's length is chosen by, so two fit one file.
const LINES_PER_FILE: usize = 2;

/// Records the rotation test writes, well past what the retained files hold.
const RECORDS_PAST_EVERY_FILE: usize = 40;

/// Records the restart test writes before it reopens the sink.
const RECORDS_BEFORE_RESTART: usize = 6;

/// Returns one row's string member.
fn text<'row>(row: &'row Value, member: &str) -> &'row str {
    row[member].as_str().unwrap_or_else(|| panic!("{member} is a string in {row}"))
}

/// Returns every row of the fixture.
fn rows() -> Vec<Value> {
    VECTORS
        .lines()
        .map(|line| serde_json::from_str(line).expect("every fixture line is one object"))
        .collect()
}

/// Returns bounds small enough for a test to reach them.
fn small_bounds() -> DiagnosticBounds {
    DiagnosticBounds {
        field_bytes: FIELD_BYTES,
        file_bytes: SMALL_FILE_BYTES,
        record_bytes: RECORD_BYTES,
        retained_files: RETAINED_FILES,
        total_bytes: SMALL_FILE_BYTES * (RETAINED_FILES as u64 + 1),
    }
}

/// Returns a directory and one sink rooted inside it.
fn sink(bounds: DiagnosticBounds) -> (tempfile::TempDir, DiagnosticSink) {
    let directory = tempfile::tempdir().expect("a directory");
    let opened =
        DiagnosticSink::open(&directory.path().join("diagnostics"), bounds).expect("a sink");
    (directory, opened)
}

/// Returns everything every diagnostic file holds.
fn everything_written(sink: &DiagnosticSink) -> String {
    let mut held = std::fs::read_to_string(sink.active_path()).unwrap_or_default();
    for ordinal in 1..=RETAINED_FILES {
        held.push_str(&std::fs::read_to_string(sink.rotated_path(ordinal)).unwrap_or_default());
    }
    held
}

#[test]
fn the_bounds_are_the_runtime_contract_s_and_the_total_is_derived() {
    let bounds = DiagnosticBounds::embedded();
    assert_eq!(bounds.record_bytes, RECORD_BYTES);
    assert_eq!(bounds.field_bytes, FIELD_BYTES);
    assert_eq!(bounds.file_bytes, FILE_BYTES);
    assert_eq!(bounds.retained_files, RETAINED_FILES);
    assert_eq!(bounds.total_bytes, TOTAL_BYTES, "which is one file's bound times the files kept");
    assert_eq!(
        bounds.file_bytes * u64::try_from(bounds.retained_files).expect("a countable bound"),
        bounds.total_bytes,
        "and the arithmetic says so rather than the manifest merely asserting it"
    );
}

#[test]
fn every_vector_keeps_what_is_worth_reading_and_loses_what_is_not() {
    let vectors = rows();
    assert!(vectors.len() >= 11, "every kind of secret, and enough ordinary sentences");
    for row in &vectors {
        let original = text(row, "text");
        let redacted = redact(original);
        let carries_a_secret = row["redacted"].as_bool().expect("a vector states its verdict");
        if carries_a_secret {
            assert!(
                redacted.contains(REDACTION) || redacted.contains(REDACTED_PATH),
                "{}: nothing was removed from {redacted}",
                text(row, "note")
            );
        } else {
            assert_eq!(redacted, original, "{}: an ordinary sentence changed", text(row, "note"));
        }
    }
}

#[test]
fn no_fixture_secret_survives_into_any_file_error_or_status() {
    let (_directory, sink) = sink(small_bounds());
    let secrets = [
        "eyJhbGciOi.secret.value",
        "p-8Kq2ZmXn",
        "hunter2",
        "ya29.A0ARrdaM",
        "MIIEvQIBADAN",
        "/home/someone/.config/slingshot/profiles.toml",
    ];
    for row in rows() {
        sink.record(text(&row, "text")).expect("a record");
    }

    let written = everything_written(&sink);
    for secret in secrets {
        assert!(
            !written.contains(secret),
            "{secret} reached a diagnostic file, which outlives the moment that produced it"
        );
    }
    let health = format!("{:?}", sink.health().expect("sink health"));
    assert!(
        !health.contains("diagnostics") || !health.contains('/'),
        "and status reports health without telling anyone where the files are: {health}"
    );
}

#[test]
fn a_record_at_its_bound_is_kept_and_a_longer_one_is_cut_where_text_allows() {
    let (_directory, sink) = sink(DiagnosticBounds { file_bytes: FILE_BYTES, ..small_bounds() });
    let exact = "a".repeat(RECORD_BYTES);
    sink.record(&exact).expect("the largest record");
    let over = "b".repeat(RECORD_BYTES + 1);
    sink.record(&over).expect("one byte further");

    let written = std::fs::read_to_string(sink.active_path()).expect("the active file reads");
    let lines: Vec<&str> = written.lines().collect();
    assert_eq!(lines[0].len(), RECORD_BYTES, "the record at the bound is whole");
    assert_eq!(lines[1].len(), RECORD_BYTES, "and the longer one was cut to it");

    let multibyte = "é".repeat(RECORD_BYTES);
    sink.record(&multibyte).expect("a record of multi-byte characters");
    let written = std::fs::read_to_string(sink.active_path()).expect("the active file reads");
    assert!(
        written.lines().all(|line| !line.is_empty()),
        "a cut lands on a character boundary, so what is written is still text"
    );
}

#[test]
fn rotation_keeps_the_named_number_of_files_and_no_more() {
    let (_directory, sink) = sink(small_bounds());
    let line =
        "c".repeat(usize::try_from(SMALL_FILE_BYTES).expect("a countable bound") / LINES_PER_FILE);
    for _ in 0..RECORDS_PAST_EVERY_FILE {
        sink.record(&line).expect("a record");
    }

    let health = sink.health().expect("sink health");
    assert!(
        health.rotated_files <= RETAINED_FILES,
        "rotation never keeps more than the named count: {health:?}"
    );
    assert!(
        health.total_bytes <= health.total_limit,
        "and the total stays inside its derived budget: {health:?}"
    );
    assert!(health.active_bytes <= SMALL_FILE_BYTES, "with an active file inside its own bound");
    assert!(
        !sink.rotated_path(RETAINED_FILES + 1).exists(),
        "and nothing beyond the bound was ever created"
    );
}

#[test]
fn a_restart_continues_the_same_rotation_rather_than_starting_over() {
    let directory = tempfile::tempdir().expect("a directory");
    let root = directory.path().join("diagnostics");
    let line =
        "d".repeat(usize::try_from(SMALL_FILE_BYTES).expect("a countable bound") / LINES_PER_FILE);
    let before = {
        let sink = DiagnosticSink::open(&root, small_bounds()).expect("a sink");
        for _ in 0..RECORDS_BEFORE_RESTART {
            sink.record(&line).expect("a record");
        }
        sink.health().expect("sink health")
    };

    let sink = DiagnosticSink::open(&root, small_bounds()).expect("a sink after a restart");
    let after = sink.health().expect("sink health");
    assert_eq!(after.rotated_files, before.rotated_files, "the rotated files are still there");
    assert_eq!(after.active_bytes, before.active_bytes, "and the active file is where it was");
    sink.record(&line).expect("a record after the restart");
    assert!(
        sink.health().expect("sink health").total_bytes <= after.total_limit,
        "with the same bound still holding"
    );
}

#[test]
fn a_directory_anyone_can_reach_is_not_a_sink() {
    let directory = tempfile::tempdir().expect("a directory");
    let root = directory.path().join("diagnostics");
    DiagnosticSink::open(&root, small_bounds()).expect("a sink");
    make_reachable_by_others(&root);

    let refused = DiagnosticSink::open(&root, small_bounds());
    assert!(
        matches!(refused, Err(DiagnosticFailure::NotPrivate)),
        "diagnostics anyone can read are diagnostics this daemon will not write: {refused:?}"
    );
}

/// Widens one directory's permissions so the sink should refuse it.
#[cfg(unix)]
fn make_reachable_by_others(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt as _;

    /// Owner everything, and read and traverse for everyone else.
    const WIDE_OPEN: u32 = 0o755;

    std::fs::set_permissions(path, std::fs::Permissions::from_mode(WIDE_OPEN))
        .expect("the permissions change");
}

/// Widens one directory's permissions so the sink should refuse it.
#[cfg(not(unix))]
fn make_reachable_by_others(_path: &std::path::Path) {}
