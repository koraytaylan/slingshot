//! Getting bytes into the store, and what is left behind when that is interrupted.
//!
//! Installation is a streaming write to a temporary file beside its
//! destination, followed by a synchronize and a rename. So the interesting
//! cases are the boundaries of the transfer loop, identical content arriving
//! twice, and what an operator finds after an interruption.

use slingshot_storage::artifact_store::{
    ArtifactFailure, CANONICAL_JSON_MEDIA_TYPE, ResultPlacement, STRUCTURED_RESULT_SLOT,
};

use crate::fixtures::*;

#[test]
fn incremental_chunks_share_verified_publication_and_refusal_cleanup() {
    let (directory, store) = store();
    let request = request(&partition(FIRST_PRINCIPAL), "incremental", "content_package");
    let digest = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    let content = directory.path().join("content");
    {
        let mut writer = store.begin_verified(&request, 3, digest).unwrap();
        assert_eq!(format!("{writer:?}"), "ArtifactStageWriter([redacted])");
        writer.write_chunk(b"a").unwrap();
        // Dropping the sink is also what cancellation of its owning future does.
    }
    assert_eq!(std::fs::read_dir(&content).unwrap().count(), 0);
    let mut writer = store.begin_verified(&request, 3, digest).unwrap();
    writer.write_chunk(b"abc").unwrap();
    assert!(writer.write_chunk(b"d").is_err());
    assert!(writer.write_chunk(b"").is_err(), "a later empty chunk cannot clear refusal");
    assert!(writer.finish().is_err());
    assert_eq!(std::fs::read_dir(&content).unwrap().count(), 0);
    for invalid in [b"ab".as_slice(), b"abd".as_slice()] {
        let mut writer = store.begin_verified(&request, 3, digest).unwrap();
        writer.write_chunk(invalid).unwrap();
        assert!(writer.finish().is_err());
        assert_eq!(std::fs::read_dir(&content).unwrap().count(), 0);
    }
    for split in 0..=3 {
        let mut writer = store.begin_verified(&request, 3, digest).unwrap();
        writer.write_chunk(&b"abc"[..split]).unwrap();
        writer.write_chunk(&b"abc"[split..]).unwrap();
        let stage = writer.finish().unwrap();
        assert_eq!(stage.metadata().byte_length, 3);
        stage.publish().unwrap();
        assert_eq!(std::fs::read(content.join(digest)).unwrap(), b"abc");
        assert_eq!(std::fs::read_dir(&content).unwrap().count(), 1);
    }
}

#[test]
fn private_verified_stage_is_unpublished_until_consumed_and_drop_removes_it() {
    let (directory, store) = store();
    let request = request(&partition(FIRST_PRINCIPAL), "staged-operation", "content_package");
    let digest = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    let content = directory.path().join("content");
    let stage = store.stage_verified(&request, &mut b"abc".as_slice(), 3, digest).unwrap();
    assert_eq!(stage.metadata().content_digest, digest);
    assert_eq!(format!("{stage:?}"), "StagedArtifact([redacted])");
    assert!(!content.join(digest).exists());
    assert!(store.open_verified(stage.metadata()).is_err());
    assert_eq!(std::fs::read_dir(&content).unwrap().count(), 1);
    drop(stage);
    assert_eq!(std::fs::read_dir(&content).unwrap().count(), 0);
    let installed = store
        .stage_verified(&request, &mut b"abc".as_slice(), 3, digest)
        .unwrap()
        .publish()
        .unwrap();
    assert_eq!(std::fs::read(content.join(digest)).unwrap(), b"abc");
    assert_eq!(installed.byte_length, 3);
    let abandoned_duplicate =
        store.stage_verified(&request, &mut b"abc".as_slice(), 3, digest).unwrap();
    drop(abandoned_duplicate);
    assert_eq!(std::fs::read_dir(&content).unwrap().count(), 1);
    assert_eq!(std::fs::read(content.join(digest)).unwrap(), b"abc");
}

#[test]
fn private_stage_reads_are_verified_and_never_create_a_public_address() {
    let (directory, store) = store();
    let request = request(&partition(FIRST_PRINCIPAL), "private-read", "content_package");
    let digest = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    let content = directory.path().join("content");
    let stage = store.stage_verified(&request, &mut b"abc".as_slice(), 3, digest).unwrap();
    let mut reader = stage.open_verified().unwrap();
    let mut bytes = [0; 2];
    assert_eq!(reader.read_into(&mut bytes).unwrap(), 2);
    assert_eq!(&bytes, b"ab");
    assert!(reader.finish().is_err(), "partial validation cannot succeed");
    let mut reader = stage.open_verified().unwrap();
    let mut bytes = [0; 4];
    assert_eq!(reader.read_into(&mut bytes).unwrap(), 3);
    assert_eq!(&bytes[..3], b"abc");
    assert_eq!(reader.read_into(&mut bytes).unwrap(), 0);
    reader.finish().unwrap();
    assert!(!content.join(digest).exists());
    assert!(store.open_verified(stage.metadata()).is_err());
    let mut reader = stage.open_verified().unwrap();
    let name = std::fs::read_dir(&content).unwrap().next().unwrap().unwrap().path();
    std::fs::write(&name, b"abd").unwrap();
    reader.read_into(&mut bytes).unwrap();
    assert!(reader.finish().is_err(), "same-length mutation must invalidate validation");
    assert!(stage.open_verified().is_err());
    assert!(stage.publish().is_err());
    assert_eq!(std::fs::read_dir(&content).unwrap().count(), 0);
}

#[test]
fn failed_stage_publication_cleans_only_private_content_and_preserves_the_destination() {
    let (directory, store) = store();
    let request = request(&partition(FIRST_PRINCIPAL), "staged-operation", "content_package");
    let digest = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    let content = directory.path().join("content");
    let stage = store.stage_verified(&request, &mut b"abc".as_slice(), 3, digest).unwrap();
    // A conflicting existing object is not ours to replace or delete.
    std::fs::write(content.join(digest), b"conflict").unwrap();
    assert!(stage.publish().is_err());
    assert_eq!(std::fs::read_dir(&content).unwrap().count(), 1);
    assert_eq!(std::fs::read(content.join(digest)).unwrap(), b"conflict");
}

#[test]
fn changed_private_stage_refuses_publication_and_replaced_name_is_not_deleted() {
    let (directory, store) = store();
    let request = request(&partition(FIRST_PRINCIPAL), "staged-operation", "content_package");
    let digest = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    let content = directory.path().join("content");
    let stage = store.stage_verified(&request, &mut b"abc".as_slice(), 3, digest).unwrap();
    let name = std::fs::read_dir(&content).unwrap().next().unwrap().unwrap().path();
    std::fs::write(&name, b"changed").unwrap();
    assert!(stage.publish().is_err());
    assert_eq!(std::fs::read_dir(&content).unwrap().count(), 0);
    let stage = store.stage_verified(&request, &mut b"abc".as_slice(), 3, digest).unwrap();
    let name = std::fs::read_dir(&content).unwrap().next().unwrap().unwrap().path();
    let displaced = directory.path().join("displaced");
    std::fs::rename(&name, &displaced).unwrap();
    std::fs::write(&name, b"replacement").unwrap();
    assert!(stage.publish().is_err());
    assert_eq!(std::fs::read(&name).unwrap(), b"replacement");
    assert!(!content.join(digest).exists());
}

#[test]
fn expected_content_is_checked_before_publication_and_refusals_leave_no_files() {
    let (directory, store) = store();
    let request = request(&partition(FIRST_PRINCIPAL), "verified-operation", "content_package");
    // Independently known SHA-256 of the three ASCII bytes abc.
    let digest = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    for (mut body, length, expected) in [
        (b"abc".as_slice(), 3, "not-a-digest"),
        (b"ab".as_slice(), 3, digest),
        (b"abcd".as_slice(), 3, digest),
        (b"abd".as_slice(), 3, digest),
    ] {
        assert!(store.install_verified(&request, &mut body, length, expected).is_err());
        assert_eq!(std::fs::read_dir(directory.path().join("content")).unwrap().count(), 0);
    }
    let mut oversized = std::io::Cursor::new(vec![b'x'; TRANSFER_BYTES * 2]);
    assert!(store.install_verified(&request, &mut oversized, 3, digest).is_err());
    assert_eq!(oversized.position(), 4, "read only the expected length plus the overshoot probe");
    let installed = store.install_verified(&request, &mut b"abc".as_slice(), 3, digest).unwrap();
    assert_eq!(installed.content_digest, digest);
    assert_eq!(installed.byte_length, 3);
    assert_eq!(std::fs::read(directory.path().join("content").join(digest)).unwrap(), b"abc");
    assert!(store.install_verified(&request, &mut b"abd".as_slice(), 3, digest).is_err());
    assert_eq!(std::fs::read(directory.path().join("content").join(digest)).unwrap(), b"abc");
    assert_eq!(std::fs::read_dir(directory.path().join("content")).unwrap().count(), 1);
}

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
fn corrupt_digest_named_content_is_refused_instead_of_deduplicated() {
    let (directory, store) = store();
    let first = store
        .install(
            &request(&partition(FIRST_PRINCIPAL), "operation-1", "content_package"),
            &mut content("one-octet").as_slice(),
        )
        .expect("the initial artifact installs");
    let destination = directory.path().join("content").join(&first.content_digest);
    std::fs::write(&destination, b"corrupt bytes").expect("the corrupt preexistence writes");
    let refused = store.install(
        &request(&partition(SECOND_PRINCIPAL), "operation-2", "content_package"),
        &mut content("one-octet").as_slice(),
    );
    assert!(
        matches!(
            refused,
            Err(ArtifactFailure::DigestMismatch { .. } | ArtifactFailure::LengthMismatch { .. })
        ),
        "a digest-shaped name is not evidence its bytes are valid: {refused:?}"
    );
    assert_eq!(std::fs::read(&destination).expect("the conflict remains"), b"corrupt bytes");
}

#[test]
fn concurrent_identical_installations_publish_one_verified_content_file() {
    let (directory, store) = store();
    let first_request = request(&partition(FIRST_PRINCIPAL), "operation-1", "content_package");
    let other_request = request(&partition(SECOND_PRINCIPAL), "operation-2", "content_package");
    let one = content("one-octet");
    let two = one.clone();
    std::thread::scope(|threads| {
        let first = threads.spawn(|| store.install(&first_request, &mut one.as_slice()));
        let second = threads.spawn(|| store.install(&other_request, &mut two.as_slice()));
        let first = first.join().expect("the first installer returns").expect("the first installs");
        let second =
            second.join().expect("the second installer returns").expect("the second installs");
        assert_eq!(first.content_digest, second.content_digest);
    });
    assert_eq!(
        std::fs::read_dir(directory.path().join("content"))
            .expect("the content directory reads")
            .count(),
        1,
        "the two publishers retain one digest-addressed file"
    );
}

#[test]
fn an_interrupted_installation_removes_its_own_unpublished_stage() {
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
    assert!(left.is_empty(), "a refused stream leaves no accumulating partial: {left:?}");
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
