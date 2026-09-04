---
id: the-artifact-store-publishes-verified-bytes
title: "The Artifact Store Publishes Verified Bytes"
workstream: "0067"
kind: task
depends_on: ["download-publication-never-replaces"]
gated: false
touches:
  - crates/slingshot-storage/src/artifact_store.rs
  - crates/slingshot-storage/tests/artifact_store/installation.rs
  - crates/slingshot-storage/tests/artifact_store/fixtures.rs
  - crates/slingshot-storage/tests/artifact_store/verification.rs
status: planned
merged_as: ""
---
# The Artifact Store Publishes Verified Bytes

A digest-named destination is treated as valid merely because it exists, parent directories are not pinned, and every directory-sync failure is swallowed. This can report successful deduplication over corrupt bytes or successful durability after the filesystem refused it.

**Steps:**

1. Open and pin owner-private store/content directories without following or later re-resolving parents; verify supported ACL/mode, type, owner, link count, and identity before relative operations.
2. Publish a verified private stage with atomic no-replace semantics. On an existing destination, read through one no-follow handle, verify ordinary-file identity and full digest/size before deduplicating; quarantine or refuse corrupt/conflicting content without overwriting it.
3. Propagate file and directory synchronization errors except a narrowly enumerated unsupported-operation result, and reconcile authenticated partials at startup or immediately after stream/oversize failures.
4. Test corrupt preexistence, destination and ancestor races, content-root substitution, hardlinks, wide ACL/mode, two concurrent identical and conflicting publishers, partial reads, and injected file-sync/directory-sync errors across reopen.

- **Done when:** install succeeds only for bytes verified under the requested digest, never replaces conflicting content, reports every real durability failure, and a crash or failed stream leaves no unauthenticated or indefinitely accumulating partial state.
