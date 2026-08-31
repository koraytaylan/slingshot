//! Reading an artifact, and every way that can fail to be a success.

use std::io::{Seek as _, Write as _};

use slingshot_storage::artifact_store::{ArtifactFailure, ArtifactMetadata};

use crate::fixtures::*;

/// Reads one artifact through to its end, returning what was streamed.
fn read_whole(
    store: &slingshot_storage::artifact_store::ArtifactStore,
    metadata: &ArtifactMetadata,
) -> Result<Vec<u8>, ArtifactFailure> {
    let mut reader = store.open_verified(metadata)?;
    let mut streamed = Vec::new();
    let mut transfer = [0_u8; READ_BYTES];
    loop {
        let read = reader.read_into(&mut transfer)?;
        if read == 0 {
            break;
        }
        streamed.extend_from_slice(&transfer[..read]);
    }
    reader.finish()?;
    Ok(streamed)
}

#[test]
fn every_content_vector_reads_back_byte_identically() {
    let (_directory, store) = store();
    for (index, row) in rows(CONTENTS).iter().enumerate() {
        let bytes = content(text(row, "file"));
        let operation = format!("operation-{index}");
        let metadata = store
            .install(
                &request(&partition(FIRST_PRINCIPAL), &operation, "content_package"),
                &mut bytes.as_slice(),
            )
            .expect("an installation");
        assert_eq!(
            read_whole(&store, &metadata).expect("a verified read"),
            bytes,
            "{}",
            text(row, "note")
        );
    }
}

#[test]
fn metadata_that_does_not_describe_the_content_is_refused_before_any_byte() {
    let (_directory, store) = store();
    let metadata = store
        .install(
            &request(&partition(FIRST_PRINCIPAL), "operation-1", "content_package"),
            &mut content("streamed").as_slice(),
        )
        .expect("an installation");

    let wrong_length =
        ArtifactMetadata { byte_length: metadata.byte_length - 1, ..metadata.clone() };
    assert!(
        matches!(store.open_verified(&wrong_length), Err(ArtifactFailure::LengthMismatch { .. })),
        "a length that is not the content's length opens nothing"
    );
    let absent =
        ArtifactMetadata { content_digest: "f".repeat(DIGEST_CHARACTERS), ..metadata.clone() };
    assert!(
        matches!(store.open_verified(&absent), Err(ArtifactFailure::NoSuchContent(_))),
        "and a digest nothing is stored under opens nothing either"
    );
    let malformed = ArtifactMetadata { content_digest: "not a digest".to_owned(), ..metadata };
    assert!(
        matches!(store.open_verified(&malformed), Err(ArtifactFailure::DigestNotCanonical)),
        "and neither does a digest that is not one"
    );
}

#[test]
fn content_mutated_before_it_is_opened_is_refused() {
    let (directory, store) = store();
    let metadata = store
        .install(
            &request(&partition(FIRST_PRINCIPAL), "operation-1", "content_package"),
            &mut content("exact-transfer").as_slice(),
        )
        .expect("an installation");

    let path = directory.path().join("content").join(&metadata.content_digest);
    let mut file = std::fs::OpenOptions::new().write(true).open(&path).expect("the content opens");
    file.write_all(b"tampered").expect("a mutation");
    file.sync_all().expect("a durable mutation");
    drop(file);

    let refused = store.open_verified(&metadata);
    assert!(
        matches!(refused, Err(ArtifactFailure::DigestMismatch { .. })),
        "the first pass reads the whole file, so a mutation is caught before a byte is emitted: \
         {refused:?}"
    );
}

#[test]
fn content_truncated_before_it_is_opened_is_refused() {
    let (directory, store) = store();
    let metadata = store
        .install(
            &request(&partition(FIRST_PRINCIPAL), "operation-1", "content_package"),
            &mut content("exact-transfer").as_slice(),
        )
        .expect("an installation");

    let path = directory.path().join("content").join(&metadata.content_digest);
    let file = std::fs::OpenOptions::new().write(true).open(&path).expect("the content opens");
    file.set_len(metadata.byte_length / HALF).expect("a truncation");
    drop(file);

    assert!(
        matches!(store.open_verified(&metadata), Err(ArtifactFailure::LengthMismatch { .. })),
        "a truncation changes the length the first pass measures"
    );
}

#[test]
fn replacing_the_path_between_the_passes_substitutes_no_bytes() {
    let (directory, store) = store();
    let original = content("exact-transfer");
    let metadata = store
        .install(
            &request(&partition(FIRST_PRINCIPAL), "operation-1", "content_package"),
            &mut original.as_slice(),
        )
        .expect("an installation");

    let reader = store.open_verified(&metadata).expect("a verified handle");
    let path = directory.path().join("content").join(&metadata.content_digest);
    let substitute = directory.path().join("content").join("substitute");
    std::fs::write(&substitute, b"entirely different bytes").expect("a substitute file");
    std::fs::rename(&substitute, &path).expect("the path now names another file");

    let mut reader = reader;
    let mut streamed = Vec::new();
    let mut transfer = [0_u8; READ_BYTES];
    loop {
        let read = reader.read_into(&mut transfer).expect("a read");
        if read == 0 {
            break;
        }
        streamed.extend_from_slice(&transfer[..read]);
    }
    reader.finish().expect("the handle is still the file that was verified");
    assert_eq!(
        streamed, original,
        "the reader holds the file it verified, not whatever the name came to mean"
    );
}

#[test]
fn content_rewritten_underneath_the_second_pass_reports_no_success() {
    let (directory, store) = store();
    let metadata = store
        .install(
            &request(&partition(FIRST_PRINCIPAL), "operation-1", "content_package"),
            &mut content("streamed").as_slice(),
        )
        .expect("an installation");

    let mut reader = store.open_verified(&metadata).expect("a verified handle");
    let mut transfer = [0_u8; READ_BYTES];
    reader.read_into(&mut transfer).expect("a first read");

    let path = directory.path().join("content").join(&metadata.content_digest);
    let mut file = std::fs::OpenOptions::new().write(true).open(&path).expect("the content opens");
    file.rewind().expect("a rewind");
    file.write_all(b"rewritten in place").expect("a rewrite");
    file.sync_all().expect("a durable rewrite");
    drop(file);

    loop {
        let read = reader.read_into(&mut transfer).expect("a read");
        if read == 0 {
            break;
        }
    }
    let refused = reader.finish();
    assert!(
        refused.is_err(),
        "a rewrite during the second pass can produce no successful end: {refused:?}"
    );
}

#[test]
fn content_truncated_during_the_second_pass_reports_no_success() {
    let (directory, store) = store();
    let metadata = store
        .install(
            &request(&partition(FIRST_PRINCIPAL), "operation-1", "content_package"),
            &mut content("streamed").as_slice(),
        )
        .expect("an installation");

    let mut reader = store.open_verified(&metadata).expect("a verified handle");
    let mut transfer = [0_u8; READ_BYTES];
    reader.read_into(&mut transfer).expect("a first read");

    let path = directory.path().join("content").join(&metadata.content_digest);
    let file = std::fs::OpenOptions::new().write(true).open(&path).expect("the content opens");
    file.set_len(u64::try_from(transfer.len()).expect("a length")).expect("a truncation");
    drop(file);

    loop {
        let read = reader.read_into(&mut transfer).expect("a read");
        if read == 0 {
            break;
        }
    }
    let refused = reader.finish();
    assert!(
        matches!(refused, Err(ArtifactFailure::LengthMismatch { .. })),
        "what was streamed is no longer the recorded length: {refused:?}"
    );
}

#[test]
fn a_resumed_transfer_still_proves_the_prefix_it_skipped() {
    let (_directory, store) = store();
    let bytes = content("streamed");
    let metadata = store
        .install(
            &request(&partition(FIRST_PRINCIPAL), "operation-1", "content_package"),
            &mut bytes.as_slice(),
        )
        .expect("an installation");

    let offset = u64::try_from(TRANSFER_BYTES).expect("a countable offset");
    let mut reader = store.open_verified(&metadata).expect("a verified handle");
    reader.discard_prefix(offset).expect("a proved prefix");
    assert_eq!(reader.transferred_bytes(), offset, "the prefix was read rather than sought over");

    let mut streamed = Vec::new();
    let mut transfer = [0_u8; READ_BYTES];
    loop {
        let read = reader.read_into(&mut transfer).expect("a read");
        if read == 0 {
            break;
        }
        streamed.extend_from_slice(&transfer[..read]);
    }
    reader.finish().expect("a verified resumed transfer");
    assert_eq!(
        streamed,
        bytes[usize::try_from(offset).expect("a countable offset")..],
        "and the suffix is exactly what follows it"
    );
}

#[test]
fn a_prefix_longer_than_the_content_is_refused() {
    let (_directory, store) = store();
    let metadata = store
        .install(
            &request(&partition(FIRST_PRINCIPAL), "operation-1", "content_package"),
            &mut content("one-octet").as_slice(),
        )
        .expect("an installation");

    let mut reader = store.open_verified(&metadata).expect("a verified handle");
    let refused = reader.discard_prefix(metadata.byte_length + 1);
    assert!(
        matches!(refused, Err(ArtifactFailure::LengthMismatch { .. })),
        "a resume past the end of the content is refused: {refused:?}"
    );
}
