---
id: workspace-scaffold
title: "Workspace Scaffold"
workstream: "0001"
kind: task
depends_on: []
gated: false
touches:
  - Cargo.toml
  - Cargo.lock
  - rust-toolchain.toml
  - rustfmt.toml
  - crates/*/Cargo.toml
  - crates/*/src/lib.rs
  - crates/slingshot-command-line/src/main.rs
  - crates/slingshot-development/src/main.rs
  - crates/slingshot-development/tests/workspace_scaffold.rs
  - "crates/slingshot-development/tests/fixtures/workspace-scaffold/**"
status: done
merged_as: "cdf3f342b813276edc983c94f2c2418f9be669d6"
---
# Workspace Scaffold

The first task creates one compilable unpublished workspace skeleton with the complete package and binary target set, leaving module ownership, external capabilities, and supported-platform policy to their own tasks.

**Steps:**

1. Author workspace-structure fixtures naming the ten packages, ten library targets, both binary targets, resolver/toolchain/edition fields, inherited lint table including exact `unsafe_code = "forbid"`, and unpublished/legal-metadata boundary before creating manifests.
2. Create the virtual root workspace, pinned Rust 1.98.0 toolchain with formatting/linting components, formatter settings, inherited workspace lint policy with no member allowance or expectation, all ten minimal member manifests/library roots, and a committed lockfile without external dependencies.
3. Add the thin product `slingshot` binary and `slingshot-development` repository-command binary; each process entry delegates to its library and the product supports only the version proof owned here.
4. Keep support crates out of product normal/build edges, set every package `publish = false`, omit license/license-file/repository fields until an owner supplies them, and inherit all other common package/lint values from the workspace.

**Tests:**

- The structure assertion compares the package set with the ten exact crate names and rejects a missing/additional member or target.
- The independent manifest parser proves root `resolver = "3"`, inherited edition 2024, Rust 1.98.0, and lint declarations; Cargo metadata separately proves the exact `slingshot` and `slingshot-development` binary targets, required library targets, editions, and Rust-version values. No test claims Cargo metadata format version 1 exposes the resolver field.
- Cargo metadata and manifest fixtures prove every member and target inherits `unsafe_code = "forbid"`, and a fixture with a member-level allow/expect override is rejected.
- Cargo metadata proves every member is unpublished and has no inferred license, license-file, or repository value; the release prerequisite fixture rejects packaging while those owner-supplied fields are absent.
- Every member compiles without an external dependency, and product manifests contain no normal/build edge to a support crate.
- `cargo run --locked -p slingshot-command-line -- --version` prints one version line and exits successfully without creating runtime files.

- **Done when:** `cargo test -p slingshot-development --test workspace_scaffold && cargo check --locked --workspace --all-targets && cargo run --locked -p slingshot-command-line -- --version` proves the exact unpublished ten-library/two-binary skeleton, inherited unsafe-code forbiddance, and legal/repository metadata boundary without any external dependency or runtime-state creation.
