//! Probe for the deterministic tape-archive capability.
//!
//! Requires writing entries whose modification time, ownership, and mode come
//! from the archive policy rather than from the filesystem, so two runs over the
//! same input produce the same bytes and the archive reads back unchanged.

use std::io::Read;

use tar::{Archive, Builder, Header};

/// Fixed modification time every deterministic archive entry carries.
const NORMALIZED_MODIFICATION_TIME: u64 = 0;

/// Fixed owner identity every deterministic archive entry carries.
const NORMALIZED_OWNER_IDENTITY: u64 = 0;

/// Fixed permission mode every deterministic archive entry carries.
const NORMALIZED_MODE: u32 = 0o644;

/// Builds one archive over the same fixed input.
fn build_archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut builder = Builder::new(Vec::new());
    for (name, contents) in entries {
        let mut header = Header::new_gnu();
        header.set_size(contents.len() as u64);
        header.set_mtime(NORMALIZED_MODIFICATION_TIME);
        header.set_uid(NORMALIZED_OWNER_IDENTITY);
        header.set_gid(NORMALIZED_OWNER_IDENTITY);
        header.set_mode(NORMALIZED_MODE);
        header.set_cksum();
        builder.append_data(&mut header, name, *contents).expect("the entry is appended");
    }
    builder.into_inner().expect("the archive finishes")
}

#[test]
fn two_runs_over_the_same_input_produce_the_same_archive_bytes() {
    let entries: [(&str, &[u8]); 2] = [("slingshot", b"executable"), ("SHA256SUMS", b"digest")];
    let first = build_archive(&entries);
    let second = build_archive(&entries);
    assert_eq!(first, second, "the archive is byte-identical across runs");

    let mut archive = Archive::new(first.as_slice());
    let mut read_back = Vec::new();
    for entry in archive.entries().expect("the archive lists entries") {
        let mut entry = entry.expect("the entry reads");
        let name = entry.path().expect("the entry names itself").display().to_string();
        let header = entry.header().clone();
        assert_eq!(header.mtime().expect("the entry has a time"), NORMALIZED_MODIFICATION_TIME);
        assert_eq!(header.mode().expect("the entry has a mode"), NORMALIZED_MODE);
        let mut contents = Vec::new();
        entry.read_to_end(&mut contents).expect("the entry contents read");
        read_back.push((name, contents));
    }
    assert_eq!(read_back.len(), entries.len());
    assert_eq!(read_back[0].0, "slingshot");
    assert_eq!(read_back[0].1, b"executable");

    let truncated = Archive::new(&first[..first.len() / 2])
        .entries()
        .expect("the reader starts")
        .map(|entry| {
            entry.and_then(|mut found| {
                let mut sink = Vec::new();
                found.read_to_end(&mut sink).map(|_| ())
            })
        })
        .collect::<Result<Vec<()>, _>>();
    assert!(truncated.is_err(), "a truncated archive must be refused");
}
