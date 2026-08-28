---
id: checksum-verified-artifact-store
title: "Checksum Verified Artifact Store"
workstream: "0015"
kind: task
depends_on:
  - sqlite-schema-and-migrations
gated: false
touches:
  - crates/slingshot-storage/src/artifact_store.rs
  - crates/slingshot-storage/tests/artifact_store.rs
  - "crates/slingshot-storage/tests/fixtures/artifacts/**"
status: planned
merged_as: ""
---
# Checksum Verified Artifact Store

Package downloads and other large results must remain outside protocol frames without becoming unverifiable filesystem paths. This task builds the content-addressed artifact boundary.

**Steps:**

1. Author byte fixtures/faults first for empty, boundary, streamed, duplicate, truncated, tampered, path replacement, concurrent in-place rewrite/truncation after first pass and during second pass, interruption, same digest, deterministic slot, same-author different opaque authentication principals, inline result, and oversized structured result.
2. Implement streaming installation through a same-directory temporary file while computing SHA-256 and byte length, followed by file synchronization, atomic rename, and supported directory synchronization.
3. Derive `ArtifactIdentifier` from version marker, installation identifier, author-target digest, operation identifier, and command-schema-declared slot; reserve deterministic `structured_result` for canonical `application/json` fallback and never derive identity from a file name or remote label.
4. Return metadata containing artifact identifier, slot, digest, exact byte length, bounded media type, and bounded descriptor rather than an absolute path or remote URL.
5. Open without following links, capture handle identity/metadata, verify digest/length through the handle, rewind it, and return a tracked second-pass reader without reopening a path. Hash/count actual streamed bytes and capture final handle metadata; expose successful completion only when both pass facts and handle identity/metadata match.
6. Store valid logical results inline only within `MAXIMUM_INLINE_MACHINE_RESULT_BYTES`; canonicalize larger valid structured results into `structured_result` through the same synchronized installation and association path.
7. Enforce current-user directory/file ownership and permissions, reuse duplicate content, and leave abandoned temporary or unreferenced files recoverable without treating them as results.

**Tests:**

- Every fixture installs and reads byte-identically with matching metadata.
- Duplicate content creates one addressed artifact.
- Mutation, truncation, and metadata mismatch are detected before any successful read result.
- Replacing a path cannot substitute bytes because both phases use one handle; concurrent in-place rewrite/truncation before or during second pass yields no successful completion because actual streamed digest/length or final metadata differs.
- Identical operation/slot inputs yield one identifier; a different target—including only a different opaque authentication principal at the same author—or slot yields another; and valid over-inline-budget structured results become verified canonical JavaScript Object Notation artifacts rather than failures or remote URLs.
- Inline values, descriptors, identifiers, media types, digests, and local-URI components satisfy their individual machine-envelope budgets, including the exact aggregate boundary below 4,096 bytes.
- Interruption at each installation boundary leaves either a complete verified artifact or an ignorable temporary file, never a partial success.

- **Done when:** `cargo test -p slingshot-storage --test artifact_store` proves deterministic target-partitioned slots, bounded result disposition, synchronized installation, one-handle two-pass actual-stream verification, concurrent mutation/truncation refusal, secure permissions, and every interruption fixture, and all workspace gates succeed.
