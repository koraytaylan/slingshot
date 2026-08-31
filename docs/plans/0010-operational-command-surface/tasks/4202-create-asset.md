---
id: create-asset
title: "Create an Asset"
workstream: "0042"
kind: task
depends_on:
  - create-asset-folder
gated: false
touches:
  - crates/slingshot-domain/src/command/create_asset.rs
  - crates/slingshot-domain/src/command/mod.rs
  - crates/slingshot-domain/tests/create_asset.rs
  - "crates/slingshot-domain/tests/fixtures/commands/create_asset/**"
status: planned
merged_as: ""
---
# Create an Asset

This is the one command in the registry that carries bytes inward, and the whole design question is what it refuses. It carries them inline under an exact bound and refuses anything larger, rather than inventing a staging protocol this commit has not proved.

**Steps:**

1. Commit canonical accepted and refused argument fixtures and exact no-effect failure documents before the implementation, one line per vector, each carrying the note that says what it proves.
2. Implement `CreateAssetCommand` with `parent_path`, a validated repository `name`, an `InlineBinaryPayload`, and an optional `metadata` property document under the existing mutation property model.
3. Refuse the encoded payload before decoding when it exceeds `MAXIMUM_INLINE_BINARY_ENCODED_BYTES`, and refuse the decoded payload when it exceeds `MAXIMUM_INLINE_BINARY_DECODED_BYTES`, so an oversized request never allocates the thing it is too large for.
4. Implement `CreateAssetResult` carrying the created address and the original rendition's byte length, bounded by `MAXIMUM_ASSET_BYTE_LENGTH` and reusing the existing nonnegative asset byte-length value.
5. Allow exactly `parent_not_found`, `parent_access_denied`, `target_already_exists`, `payload_rejected`, `payload_too_large`, `media_type_unsupported`, `repository_commit_failed`, and `mutation_outcome_unknown`.
6. Supply request-context validation that refuses a result whose address is not the computed target and whose reported length is not the decoded payload's length.

**Tests:**

- Every accepted vector round-trips byte-identically and computes the target the fixture states.
- The encoded bound is accepted exactly and refused by one byte, before any decoding happens, proved by a vector whose encoded form is over the bound and whose decoded form would be under it.
- The decoded bound is accepted exactly and refused by one byte.
- A payload with a non-alphabet character, absent padding, doubled padding, or an interior line break is refused.
- A result whose reported byte length differs from the decoded payload's length is refused.
- Each failure document carries exactly its discriminator and `target_path` and proves no effect.

- **Done when:** `cargo test -p slingshot-domain --test create_asset` proves both payload bounds on both sides, the before-decoding order, the length echo, every closed failure, and request-context validation.
