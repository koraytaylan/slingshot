---
id: pinned-coverage-fuzzing-tool
title: "Pinned Coverage Fuzzing Tool"
workstream: "0035"
kind: task
depends_on: []
gated: false
touches:
  - fuzz/rust-toolchain.toml
  - compatibility/coverage-fuzzing.toml
  - schemas/compatibility/coverage-fuzzing-tool.schema.json
  - scripts/prepare_coverage_fuzzing_tool
  - scripts/verify_coverage_fuzzing_tool
  - crates/slingshot-development/src/coverage_fuzzing_tool.rs
  - crates/slingshot-development/src/lib.rs
  - crates/slingshot-development/src/main.rs
  - crates/slingshot-development/tests/coverage_fuzzing_tool.rs
  - "crates/slingshot-development/tests/fixtures/coverage-fuzzing-tool/**"
status: done
merged_as: "48ba05e71250a1b7c7483c9c03b5a4bdd5206778"
---
# Pinned Coverage Fuzzing Tool

Coverage evidence must come from an isolated executable whose source, toolchain, locked dependencies, build, and bytes are explicit rather than whichever `cargo-fuzz` happens to be installed on `PATH`.

**Steps:**

1. Commit the schema and accepted/rejected fixtures before implementation. Commit `compatibility/coverage-fuzzing.toml` with the canonical `https://github.com/rust-fuzz/cargo-fuzz` source repository, one full source commit, canonical source-tree and `Cargo.lock` digests, exact package/version and binary name, one dated fuzz-target nightly in `fuzz/rust-toolchain.toml`, the exact tool-build Rust/Cargo identities, allowed registry/source/checksum policy, and named bundle count/path/entry/byte limits. Branches, tags, shortened commits, mutable package selectors, absent checksums, and PATH discovery are invalid.
2. Implement a network-enabled `prepare-coverage-fuzzing-tool` development command and thin script that accept only one new empty output directory and an explicit matching supported host row. Starting with new private Cargo/source/target homes and a cleared environment, acquire only the manifest's exact source commit and locked dependency graph, disable credential persistence, verify repository/origin/commit/tree/lock plus every registry checksum before use, and reject Git dependencies, source replacement, redirects to another authority, ambient Cargo/Rust homes, installed tools, compiler wrappers, or preexisting output.
3. Build the tool twice from independent verified source exports with the exact declared build toolchain, closed environment, offline frozen second-stage Cargo inputs, and separate writable targets. Recompute both source digests, require byte-identical executables, and emit a canonical schema-valid bundle manifest binding repository, commit, tree, lock, dependency-cache content, toolchain, host/target row, closed environment, both build invocations, and executable digest without absolute paths or timestamps.
4. Implement an offline bounded bundle verifier and thin script. It rejects every missing, extra, duplicate, noncanonical, escaping, linked, special, over-bound, writable, wrong-host, wrong-toolchain, wrong-source/lock/cache, or modified executable entry before running the bundled executable by its verified absolute path and checking its exact version output.
5. Require every coverage wrapper consumer to receive an explicit verified bundle directory and resolve the executable only from its manifest. The verifier returns a path-only value; it never installs globally, mutates a Cargo home, searches `PATH`, or accepts a version string without matching source/binary provenance. Task `release-input-cache` later admits this exact bundle into the coordinator's offline cache without rebuilding or rediscovering it.

**Tests:**

- Full source commit/tree/lock, registry checksums, tool-build and target-nightly identities, supported host row, two matching build digests, and exact version output are mandatory and canonical.
- A branch/tag/short commit, wrong origin/tree/lock/checksum/version/nightly/host, mutable registry response, source replacement, compiler wrapper, ambient Cargo home, installed PATH binary, one-build manifest, unequal builds, modified executable, and missing/extra/escaping/special/over-bound bundle entry each fail with a distinct diagnostic.
- Network acquisition is confined to the preparation command's declared sources. After preparation, verification and invocation succeed with network denied, empty Cargo/Rust homes, an empty PATH, and no globally installed `cargo-fuzz`.
- Repeating preparation for one exact host and input closure yields the same executable and digest-bearing manifest content; absolute temporary roots and timestamps are absent.

- **Done when:** `cargo test -p slingshot-development --test coverage_fuzzing_tool`, `scripts/prepare_coverage_fuzzing_tool --platform-row <matching-row> --output-directory <new-bundle>`, and network-denied `scripts/verify_coverage_fuzzing_tool --tool-bundle <new-bundle>` prove exact pinned acquisition, two-build binary provenance, offline verification, and explicit-path execution with no ambient or globally installed coverage tool.
