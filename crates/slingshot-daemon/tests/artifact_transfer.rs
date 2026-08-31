//! Streaming a verified artifact, where only the end says it worked.
//!
//! The ordering is the guarantee. A client that receives chunks and then a
//! failure knows to discard them; one that receives an end knows every byte it
//! got was verified on the way past. These prove there is no arrangement of
//! responses in which a client is told a transfer succeeded and is wrong.

use serde_json::Value;
use slingshot_daemon::artifact_transfer::{
    ArtifactTransfer, TransferFailure, TransferRequest, chunk_bytes, maximum_chunk_bytes,
};
use slingshot_domain::installation::InstallationIdentifier;
use slingshot_storage::artifact_store::{ArtifactMetadata, ArtifactStore, InstallationRequest};

/// Transfer fixtures this test reads.
const CONTENTS: &str = include_str!("fixtures/artifact-transfer/contents.jsonl");

/// Two-character pairs in a sixty-four-character hexadecimal value.
const DIGEST_PAIRS: usize = 32;

/// Characters a sixty-four-character hexadecimal value has.
const DIGEST_CHARACTERS: usize = 64;

/// Bytes one chunk may carry, from the runtime contract.
const CHUNK_BYTES: u64 = 65_536;

/// A chunk length small enough to make a fixture produce many chunks.
const SMALL_CHUNK_BYTES: u64 = 1024;

/// A preference twice the bound, for proving the bound wins.
const OVER_THE_BOUND: u64 = CHUNK_BYTES * 2;

/// The smallest chunk a transfer makes progress with.
const SMALLEST_PROGRESS: u64 = 1;

/// The one chunk at the end of a transfer that may be short.
const LAST_CHUNK: usize = 1;

/// Returns one row's string member.
fn text<'row>(row: &'row Value, member: &str) -> &'row str {
    row[member].as_str().unwrap_or_else(|| panic!("{member} is a string in {row}"))
}

/// Returns every row of the fixture.
fn rows() -> Vec<Value> {
    CONTENTS
        .lines()
        .map(|line| serde_json::from_str(line).expect("every fixture line is one object"))
        .collect()
}

/// Returns the bytes one fixture holds.
fn content(file: &str) -> Vec<u8> {
    let directory =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/artifact-transfer");
    std::fs::read(directory.join(file)).expect("a fixture reads")
}

/// Returns a directory and one store rooted inside it.
fn store() -> (tempfile::TempDir, ArtifactStore) {
    let directory = tempfile::tempdir().expect("a directory");
    let store = ArtifactStore::open(directory.path()).expect("a store");
    (directory, store)
}

/// Installs one fixture's bytes and returns their metadata.
fn installed(store: &ArtifactStore, file: &str, index: usize) -> ArtifactMetadata {
    let request = InstallationRequest {
        artifact_slot: "content_package".to_owned(),
        author_target_identity_digest: "1d".repeat(DIGEST_PAIRS),
        descriptor: None,
        installation_identifier: InstallationIdentifier::parse(&"a1".repeat(DIGEST_PAIRS))
            .expect("a legal identifier"),
        media_type: "application/zip".to_owned(),
        operation_identifier: format!("operation-{index}"),
    };
    store.install(&request, &mut content(file).as_slice()).expect("an installation")
}

/// Runs one whole transfer, returning every chunk it emitted.
fn transferred(
    store: &ArtifactStore,
    metadata: &ArtifactMetadata,
    offset: u64,
    preferred: u64,
) -> Result<Vec<Vec<u8>>, TransferFailure> {
    let request = TransferRequest {
        expected_content_digest: metadata.content_digest.clone(),
        starting_offset: offset,
        preferred_chunk_bytes: preferred,
    };
    let (mut transfer, start) = ArtifactTransfer::open(store, metadata, &request)?;
    assert_eq!(start.content_digest, metadata.content_digest, "the start promises this content");
    assert_eq!(start.total_byte_length, metadata.byte_length, "and says how large it is");
    assert_eq!(
        start.transferred_byte_length,
        metadata.byte_length - offset,
        "and how much of it is coming"
    );
    let mut chunks = Vec::new();
    while let Some(chunk) = transfer.next_chunk()? {
        chunks.push(chunk);
    }
    transfer.finish()?;
    Ok(chunks)
}

#[test]
fn the_chunk_bound_is_the_manifest_s_and_a_larger_request_gets_the_bound() {
    assert_eq!(maximum_chunk_bytes(), CHUNK_BYTES);
    assert_eq!(chunk_bytes(SMALL_CHUNK_BYTES), SMALL_CHUNK_BYTES, "a modest preference is kept");
    assert_eq!(
        chunk_bytes(OVER_THE_BOUND),
        CHUNK_BYTES,
        "and a client asking for more gets the bound, because it said what it would prefer"
    );
    assert_eq!(chunk_bytes(0), SMALLEST_PROGRESS, "while asking for nothing still makes progress");
}

#[test]
fn every_fixture_streams_back_byte_identically_in_bounded_chunks() {
    let (_directory, store) = store();
    for (index, row) in rows().iter().enumerate() {
        let metadata = installed(&store, text(row, "file"), index);
        let chunks = transferred(&store, &metadata, 0, CHUNK_BYTES).expect("a verified transfer");
        let streamed: Vec<u8> = chunks.iter().flatten().copied().collect();
        assert_eq!(streamed, content(text(row, "file")), "{}", text(row, "note"));
        assert!(
            chunks
                .iter()
                .all(|chunk| u64::try_from(chunk.len()).unwrap_or(u64::MAX) <= CHUNK_BYTES),
            "{}: no chunk crosses the bound",
            text(row, "note")
        );
        assert!(
            chunks
                .iter()
                .rev()
                .skip(LAST_CHUNK)
                .all(|chunk| { u64::try_from(chunk.len()).unwrap_or_default() == CHUNK_BYTES }),
            "{}: every chunk but the last is full",
            text(row, "note")
        );
    }
}

#[test]
fn a_resumed_transfer_sends_the_suffix_and_still_proves_the_prefix() {
    let (_directory, store) = store();
    let row = rows()
        .into_iter()
        .find(|row| text(row, "name") == "several-chunks")
        .expect("a multi-chunk fixture");
    let metadata = installed(&store, text(&row, "file"), 0);
    let whole = content(text(&row, "file"));

    for offset in [0, SMALLEST_PROGRESS, CHUNK_BYTES, metadata.byte_length] {
        let chunks =
            transferred(&store, &metadata, offset, CHUNK_BYTES).expect("a verified transfer");
        let streamed: Vec<u8> = chunks.iter().flatten().copied().collect();
        assert_eq!(
            streamed,
            whole[usize::try_from(offset).expect("a countable offset")..],
            "resuming at {offset} sends exactly what follows it"
        );
    }
}

#[test]
fn a_resume_at_the_end_sends_nothing_and_still_verifies() {
    let (_directory, store) = store();
    let row = rows()
        .into_iter()
        .find(|row| text(row, "name") == "exact-chunk")
        .expect("a one-chunk fixture");
    let metadata = installed(&store, text(&row, "file"), 0);

    let chunks = transferred(&store, &metadata, metadata.byte_length, CHUNK_BYTES)
        .expect("a verified transfer of nothing");
    assert!(chunks.is_empty(), "there is nothing after the end to send");

    let past =
        transferred(&store, &metadata, metadata.byte_length + SMALLEST_PROGRESS, CHUNK_BYTES);
    assert!(
        matches!(past, Err(TransferFailure::OffsetPastEnd { .. })),
        "while resuming past the end is refused rather than answered with nothing: {past:?}"
    );
}

#[test]
fn a_caller_expecting_other_content_is_refused_before_a_handle_is_opened() {
    let (_directory, store) = store();
    let metadata = installed(&store, "one-octet.bin", 0);
    let request = TransferRequest {
        expected_content_digest: "f".repeat(DIGEST_CHARACTERS),
        starting_offset: 0,
        preferred_chunk_bytes: CHUNK_BYTES,
    };
    let refused = ArtifactTransfer::open(&store, &metadata, &request);
    assert!(
        matches!(refused, Err(TransferFailure::DigestMismatch { .. })),
        "a client asking for something else gets nothing rather than the wrong bytes: {refused:?}"
    );
}

#[test]
fn content_rewritten_mid_transfer_never_reaches_a_successful_end() {
    let (directory, store) = store();
    let row = rows()
        .into_iter()
        .find(|row| text(row, "name") == "several-chunks")
        .expect("a multi-chunk fixture");
    let metadata = installed(&store, text(&row, "file"), 0);

    let request = TransferRequest {
        expected_content_digest: metadata.content_digest.clone(),
        starting_offset: 0,
        preferred_chunk_bytes: SMALL_CHUNK_BYTES,
    };
    let (mut transfer, _start) =
        ArtifactTransfer::open(&store, &metadata, &request).expect("a verified handle");
    transfer.next_chunk().expect("a first chunk");

    let path = directory.path().join("content").join(&metadata.content_digest);
    std::fs::write(&path, b"rewritten while it was being read").expect("a rewrite");

    while transfer.next_chunk().expect("a chunk").is_some() {}
    let refused = transfer.finish();
    assert!(
        refused.is_err(),
        "chunks may have gone out, and the end is where the client learns not to trust them: \
         {refused:?}"
    );
}
