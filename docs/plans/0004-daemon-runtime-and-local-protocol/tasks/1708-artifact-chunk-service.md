---
id: artifact-chunk-service
title: "Artifact Chunk Service"
workstream: "0017"
kind: task
depends_on:
  - local-operation-envelopes
  - operation-status-and-result
  - terminal-operation-maintenance
gated: false
touches:
  - crates/slingshot-daemon/src/artifact_transfer.rs
  - crates/slingshot-daemon/tests/artifact_transfer.rs
  - "crates/slingshot-daemon/tests/fixtures/artifact-transfer/**"
status: done
merged_as: "0ebe98c93c6f7542eb3e2a84d7553c7fd3ac9b1b"
---
# Artifact Chunk Service

Large verified operation artifacts and operation-free maintenance results need a bounded local transfer path that survives client interruption without exposing daemon filesystem paths or placing the whole payload in one protocol frame.

**Steps:**

1. Commit metadata and chunk fixtures first for empty/single/multiple/exact/final, repeat/resume at offsets zero, one, length-minus-one, and length, structured result, both maintenance-result kinds, metadata/read unchanged, metadata then exact current-preview-to-application-receipt transfer, transfer after read start, missing/wrong target/operation/artifact/maintenance identity, superseded/retired maintenance association before either call, invalid owner/revision transitions, invalid offset/bounds, path replacement, mutation in the discarded prefix, concurrent in-place rewrite/truncation after first pass and during chunks, stored truncation, and corruption.
2. Resolve `MaintenanceResultMetadata` only through its explicit target digest and `MaintenanceResultIdentifier`. In one repository snapshot, rederive the identifier and verify the current readable association's kind, reviewed source digest, content digest, exact length, fixed `application/json`, association revision, live owner, and referenced blob metadata; return exactly those association facts and no bytes/path/operation/artifact/slot. Resolve an `ArtifactRead` through target digest, terminal operation, `ArtifactIdentifier`, slot, and digest. Resolve a `MaintenanceResultRead` atomically at read start through its explicit target digest, `MaintenanceResultIdentifier`, metadata-supplied or caller-supplied expected digest, current association facts, and live owner, with no operation or slot. In either read case receive the single verified/rewound tracked handle from `ArtifactStore` without returning/reopening its path.
3. On the same rewound handle, hash/count and discard `[0, starting_offset)` without emitting a response; only after the exact prefix and unchanged handle snapshot may the matching artifact- or maintenance-result-start be sent. A maintenance start repeats all current association metadata. Continue the same full second-pass hash/count while streaming ordered manifest-bounded Base64 suffix chunks of the matching response family with exact decoded length/next offset, checking decoded-chunk and inherited encoded-frame limits before every allocation/read/write. The maintenance stream linearizes at its atomic read start; a later lifecycle transaction does not change that authenticated same-handle snapshot.
4. Compare second-pass digest/length plus before/after handle identity/metadata and emit the matching successful end only on an exact match. On mutation, terminate with corruption and no success end even when provisional chunks were sent.
5. Make identical stable read requests byte-identical and accept starting offset at total length as start/end without data only after complete-artifact verification.
6. Keep metadata/read calls connection-independent and reject missing/corrupt/mutated material without changing terminal operation or maintenance association; clients stage chunks and independently verify full digest/length after success end before publish. Between metadata and read start, clients require every identity-bound metadata field to remain exact and accept only an unchanged owner/revision or current-preview to application-receipt ownership at the checked next revision. A superseded preview or retired receipt before read start has no readable association and returns the closed no-start refusal on every connection/restart; no client inverts the identifier hash to obtain the expected digest.

**Tests:**

- Every valid nonfinal chunk has the requested exact decoded length, the final chunk has the exact remainder, and all encoded frames stay below their independent bound.
- Repeating any read returns an identical response sequence; reconnecting at a committed staging length reconstructs one artifact without overlap or omission.
- Wrong target, operation, artifact identifier, slot, maintenance-result identifier/kind/source/content/length/media/owner/revision, digest, offset, requested decoded length, missing bytes, superseded/retired association, and corruption return distinct bounded failures and no internal path; a failed metadata lookup returns no partial fields and a failed read returns no start.
- Path replacement cannot substitute another handle; mutation in a discarded prefix and concurrent in-place mutation/truncation during a suffix produce no successful end and staged bytes never publish.
- Interrupted streams leave terminal operation and artifact facts unchanged and do not allocate a server-side resume record.

- **Done when:** `cargo test -p slingshot-daemon --test artifact_transfer` reconstructs every operation artifact and retained maintenance result through authenticated target-and-identifier metadata lookup, exact manifest-bounded chunks, and one tracked handle, proves the unchanged/one-transfer lookup-to-start rule plus prefix hash/discard at offsets zero/one/length-minus-one/length across restart, emits success end only after complete actual-stream/metadata verification, rejects every prefix/suffix mutation and operation-free identity/retention/integrity case, and proves independently verified staging before publication, and all workspace gates succeed.
