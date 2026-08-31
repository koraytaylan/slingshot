---
id: owner-supplied-release-metadata
title: "Owner-Supplied Release Metadata"
workstream: "0038"
kind: task
depends_on:
  - pinned-coverage-fuzzing-tool
  - daemon-process-chaos
  - minimum-rust-and-dependency-gates
  - owner-confirmed-github-automation
gated: true
touches:
  - Cargo.toml
  - "crates/*/Cargo.toml"
  - LICENSE
  - support/release-metadata.toml
  - crates/slingshot-development/src/lib.rs
  - crates/slingshot-development/src/main.rs
  - crates/slingshot-development/src/release_metadata.rs
  - crates/slingshot-development/tests/release_metadata.rs
  - "crates/slingshot-development/tests/fixtures/release-metadata/**"
status: blocked
merged_as: ""
---
# Owner-Supplied Release Metadata

Release archives require a legal declaration and exact license material that only the repository owner can choose. This gate applies the canonical repository address already fixed by the owner-confirmed automation authority and never re-infers or independently redefines it.

**Steps:**

1. Keep this task gated until the owner supplies the exact license declaration and complete `LICENSE` bytes; require the already validated canonical repository address and immutable repository identity from `support/github-automation-authority.toml`.
2. Commit accepted and rejected fixtures for an SPDX expression with declared license material, a license-file declaration, missing or empty material, conflicting Cargo fields, placeholder values, unsafe material paths, mismatched repository addresses, and publishable packages before implementation.
3. Define closed `support/release-metadata.toml` data that names exactly one owner-selected Cargo license declaration, the repository-owned `LICENSE` material and digest, and the exact canonical repository address/immutable identifier copied from the validated automation authority.
4. Apply the supplied declaration and authoritative repository address through workspace package inheritance while retaining `publish = false` for every member; do not rewrite the owner-supplied license text.
5. Implement a development validator that compares the closed metadata document, root and inherited Cargo metadata, exact license-material digest, safe repository-relative archive path, and unpublished-package state.
6. Make every packaging and release command refuse missing, unvalidated, inferred, placeholder, or changed release metadata before compiling a binary.

**Tests:**

- The fixture suite accepts only one explicit owner-selected license representation and rejects absent, conflicting, placeholder, empty, path-escaping, digest-mismatched, or inferred values.
- Cargo metadata exposes the exact supplied license or license-file declaration and the automation authority's canonical repository address through every member while every package remains unpublished.
- The validator treats the committed `LICENSE` bytes as opaque owner material, verifies their declared digest, and never synthesizes or edits them.
- Removing or changing any declared value makes release preflight fail before build or archive creation.

- **Done when:** `cargo test -p slingshot-development --test release_metadata` proves the gated owner decision is represented exactly in Cargo and the closed release manifest, every package remains unpublished, and packaging refuses any missing, inferred, placeholder, or changed legal/repository input.
