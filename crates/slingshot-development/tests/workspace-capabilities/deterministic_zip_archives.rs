//! Probe for the deterministic zip-archive capability.
//!
//! Requires entries whose modification time and permissions come from the
//! archive policy rather than from the filesystem, so two runs over the same
//! input produce the same bytes and the archive reads back unchanged.

use std::io::{Cursor, Read, Write};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipArchive, ZipWriter};

/// Fixed permission mode every deterministic archive entry carries.
const NORMALIZED_MODE: u32 = 0o644;

/// Builds one archive over the same fixed input.
fn build_archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .last_modified_time(DateTime::default())
        .unix_permissions(NORMALIZED_MODE);
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, contents) in entries {
        writer.start_file(*name, options).expect("the entry starts");
        writer.write_all(contents).expect("the entry contents are written");
    }
    writer.finish().expect("the archive finishes").into_inner()
}

/// Members the probe archive holds.
const ARCHIVE_MEMBER_COUNT: usize = 2;

/// Fraction of an archive a truncated reader receives.
const TRUNCATION_DIVISOR: usize = 2;

#[test]
fn two_runs_over_the_same_input_produce_the_same_archive_bytes() {
    let entries: [(&str, &[u8]); ARCHIVE_MEMBER_COUNT] =
        [("slingshot.exe", b"executable"), ("SHA256SUMS", b"digest")];
    let first = build_archive(&entries);
    let second = build_archive(&entries);
    assert_eq!(first, second, "the archive is byte-identical across runs");

    let mut archive = ZipArchive::new(Cursor::new(first.clone())).expect("the archive opens");
    assert_eq!(archive.len(), entries.len());
    let names: Vec<String> = archive.file_names().map(str::to_owned).collect();
    assert!(names.contains(&"slingshot.exe".to_owned()), "{names:?}");
    let mut member = archive.by_name("SHA256SUMS").expect("the member is found");
    let mut contents = Vec::new();
    member.read_to_end(&mut contents).expect("the member reads");
    assert_eq!(contents, b"digest");
    drop(member);

    let truncated =
        ZipArchive::new(Cursor::new(first[..first.len() / TRUNCATION_DIVISOR].to_vec()));
    assert!(truncated.is_err(), "a truncated archive must be refused");
}
