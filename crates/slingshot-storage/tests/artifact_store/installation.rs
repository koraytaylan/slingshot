//! Getting bytes into the store, and what is left behind when that is interrupted.
//!
//! Installation is a streaming write to a temporary file beside its
//! destination, followed by a synchronize and a rename. So the interesting
//! cases are the boundaries of the transfer loop, identical content arriving
//! twice, and what an operator finds after an interruption.

use slingshot_storage::artifact_store::{
    ArtifactFailure, CANONICAL_JSON_MEDIA_TYPE, ResultPlacement, STAGING_SUFFIX,
    STRUCTURED_RESULT_SLOT,
};

use crate::fixtures::*;

#[test]
fn every_content_vector_installs_as_the_bytes_the_fixture_measured() {
    let (_directory, store) = store();
    let vectors = rows(CONTENTS);
    assert!(vectors.len() >= 9, "empty, both transfer boundaries, and both inline boundaries");
    for (index, row) in vectors.iter().enumerate() {
        let bytes = content(text(row, "file"));
        let operation = format!("operation-{index}");
        let installed = store
            .install(
                &request(&partition(FIRST_PRINCIPAL), &operation, "content_package"),
                &mut bytes.as_slice(),
            )
            .expect("an installation");
        assert_eq!(installed.content_digest, text(row, "content_digest"), "{}", text(row, "note"));
        assert_eq!(
            installed.byte_length,
            row["byte_length"].as_u64().expect("a length"),
            "{}: the exact length, not a rounded one",
            text(row, "note")
        );
    }
}

#[test]
fn identical_content_becomes_one_addressed_artifact() {
    let (directory, store) = store();
    let first = store
        .install(
            &request(&partition(FIRST_PRINCIPAL), "operation-1", "content_package"),
            &mut content("one-octet").as_slice(),
        )
        .expect("an installation");
    let second = store
        .install(
            &request(&partition(SECOND_PRINCIPAL), "operation-2", "content_package"),
            &mut content("duplicate-of-one-octet").as_slice(),
        )
        .expect("another operation installing the same bytes");

    assert_eq!(
        first.content_digest, second.content_digest,
        "the same bytes address the same content"
    );
    assert_ne!(
        first.artifact_identifier, second.artifact_identifier,
        "while the two operations still name two artifacts"
    );
    let files: Vec<String> = std::fs::read_dir(directory.path().join("content"))
        .expect("the content directory reads")
        .map(|entry| entry.expect("an entry").file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(files, vec![first.content_digest.clone()], "and one file holds them");
}

#[test]
fn an_interrupted_installation_leaves_something_nothing_reads() {
    /// A reader that stops part way, the way an interrupted transfer does.
    struct Interrupted {
        /// Bytes still to hand out before failing.
        remaining: usize,
    }
    impl std::io::Read for Interrupted {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            if self.remaining == 0 {
                return Err(std::io::Error::other("the transfer stopped"));
            }
            let handed = self.remaining.min(buffer.len());
            self.remaining -= handed;
            buffer[..handed].fill(b'x');
            Ok(handed)
        }
    }

    let (directory, store) = store();
    let refused = store.install(
        &request(&partition(FIRST_PRINCIPAL), "operation-1", "content_package"),
        &mut Interrupted { remaining: TRANSFER_BYTES + 1 },
    );
    assert!(
        matches!(refused, Err(ArtifactFailure::FilesystemRefused(_))),
        "an interrupted installation is a refusal: {refused:?}"
    );
    let left: Vec<String> = std::fs::read_dir(directory.path().join("content"))
        .expect("the content directory reads")
        .map(|entry| entry.expect("an entry").file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(left.len(), 1, "the partial write is still there to be found: {left:?}");
    assert!(
        left[0].ends_with(STAGING_SUFFIX),
        "wearing the staging suffix, so nothing addresses it as content: {left:?}"
    );
    assert_eq!(
        std::fs::metadata(directory.path().join("content").join(&left[0]))
            .expect("the staged file reads")
            .len(),
        u64::try_from(TRANSFER_BYTES + 1).expect("a countable length"),
        "holding exactly what arrived before the transfer stopped"
    );
}

#[test]
fn a_result_within_the_inline_budget_travels_inside_the_response() {
    let (_directory, store) = store();
    let largest = "a".repeat(INLINE_RESULT_BYTES);
    let placed = store
        .place_structured_result(
            &request(&partition(FIRST_PRINCIPAL), "operation-1", "ignored"),
            &largest,
        )
        .expect("a placement");
    assert_eq!(
        placed,
        ResultPlacement::Inline(largest),
        "the largest result that fits travels whole"
    );
}

#[test]
fn a_result_above_the_inline_budget_becomes_a_verified_artifact() {
    let (_directory, store) = store();
    let over = "a".repeat(INLINE_RESULT_BYTES + 1);
    let placed = store
        .place_structured_result(
            &request(&partition(FIRST_PRINCIPAL), "operation-1", "ignored"),
            &over,
        )
        .expect("a placement");
    let ResultPlacement::Externalized(metadata) = placed else {
        panic!("one byte above the budget is externalized, not refused: {placed:?}");
    };
    assert_eq!(
        metadata.artifact_slot, STRUCTURED_RESULT_SLOT,
        "under the slot every command reserves for exactly this"
    );
    assert_eq!(
        metadata.media_type, CANONICAL_JSON_MEDIA_TYPE,
        "as canonical JavaScript Object Notation"
    );
    assert_eq!(metadata.byte_length, over.len() as u64, "holding the whole result");

    let mut reader = store.open_verified(&metadata).expect("a verified handle");
    let mut read_back = Vec::new();
    let mut transfer = [0_u8; READ_BYTES];
    loop {
        let read = reader.read_into(&mut transfer).expect("a read");
        if read == 0 {
            break;
        }
        read_back.extend_from_slice(&transfer[..read]);
    }
    reader.finish().expect("a verified transfer");
    assert_eq!(read_back, over.as_bytes(), "and it reads back byte for byte");
}

#[test]
fn a_result_above_the_largest_canonical_one_is_refused_rather_than_installed() {
    let (directory, store) = store();
    let largest = usize::try_from(CANONICAL_STRUCTURED_RESULT_BYTES).expect("a countable bound");
    let over = "a".repeat(largest + 1);
    let refused = store.place_structured_result(
        &request(&partition(FIRST_PRINCIPAL), "operation-1", "ignored"),
        &over,
    );
    assert!(
        matches!(
            refused,
            Err(ArtifactFailure::ContentTooLong { allowed, .. })
                if allowed == CANONICAL_STRUCTURED_RESULT_BYTES
        ),
        "a result no canonical form may reach is refused: {refused:?}"
    );
    assert_eq!(
        std::fs::read_dir(directory.path().join("content"))
            .expect("the content directory reads")
            .count(),
        0,
        "and nothing was written on the way to refusing it"
    );
}

/// Only the Unix platforms report the ownership and permission bits this
/// asserts on; elsewhere the store has nothing to check and nothing to refuse.
#[cfg(unix)]
#[test]
fn installed_content_is_reachable_by_its_owner_alone() {
    let (directory, store) = store();
    let metadata = store
        .install(
            &request(&partition(FIRST_PRINCIPAL), "operation-1", "content_package"),
            &mut content("one-octet").as_slice(),
        )
        .expect("an installation");
    let path = directory.path().join("content").join(&metadata.content_digest);
    require_owner_only(&path);

    make_reachable_by_others(&path);
    let refused = store.open_verified(&metadata);
    assert!(
        matches!(refused, Err(ArtifactFailure::NotPrivate)),
        "content anyone can reach is not content this store will read: {refused:?}"
    );
}
