---
id: quality-automation
title: "Quality Automation"
workstream: "0002"
kind: chore
depends_on:
  - dependency-direction-check
  - platform-runtime-contract
  - rustsec-advisory-pin
  - source-policy-checker
  - walking-skeleton-process-proof
gated: false
touches:
  - support/repository-tools.toml
  - deny.toml
  - scripts/quality
status: done
merged_as: "3b374c10adb78bacdec98ebe5f8fc37dbc32a643"
---
# Quality Automation

One checked-in command sequence defines the repository-local evidence consumed unchanged by any later owner-confirmed hosted adapter.

**Steps:**

1. Commit `support/repository-tools.toml` with exact versions and install provenance/checksums for every external foundation-gate executable, including dependency policy/advisory tools and the executable-script linter; make `scripts/quality` verify them before checks. Plan 0009's owner-confirmed evidence task owns selection and addition of its offline attestation verifier.
2. Add `scripts/quality` as a fail-fast argument-free runner whose Rust graph gates are exactly `cargo fmt --all --check`, `cargo check --locked --workspace --all-targets --all-features`, `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`, `cargo test --locked --workspace --all-targets --all-features`, and `RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --all-features --no-deps`, followed by pinned script linting, dependency direction, source/migration policy, and dependency advisory/license/source/duplicate checks. Require the explicit RustSec directory and authenticate its exact origin/full commit/clean tree before advisory analysis; never fetch or discover a cache and never label the snapshot currently fresh.
3. Configure dependency policy with an explicit accepted-license set for dependencies and rejection of unknown registries and Git sources unless repository policy names them; this dependency policy does not infer or substitute a Slingshot package license.
4. Add cold/warm repository-local harness fixtures that invoke the same argument-free script with an explicitly supplied exact RustSec checkout and prepared tool directory, never generate committed files, validate every abstract platform row through policy fakes, and run native capability/runtime/walking commands only for the row matching the current environment. A cross-compile or copied report never becomes native support; hosted all-row adapters remain outside this plan until Plan 0009's owner-confirmed provider gate.

**Tests:**

- The local script succeeds from a clean checkout and leaves `git status --short` empty.
- Harness contract tests verify pinned toolchain/tools plus the exact advisory origin/commit/tree gate, locked dependencies, one argument-free script entry, cold/warm equivalence, deterministic policy fixtures for every declared platform row, and at most the one real native capability/runtime/walking report matching the current environment.
- Dependency-policy fixtures reject an unapproved license, registry, Git source, advisory, and duplicate version with the responsible package named.
- Removing any required repository-specific development command from the script makes the contract test fail.
- Removing `--locked`, `--workspace`, `--all-targets`, `--all-features`, warnings denial, or rustdoc warnings denial from its applicable exact command makes the contract test fail.
- Passing any positional argument makes the quality script fail with its pinned usage diagnostic; only the explicit environment directory can select an audit checkout.
- Missing/wrong external tool version, unset advisory directory, advisory origin/commit/tree failure, dirty advisory checkout, an attempted timestamp/freshness assertion, and an undocumented matrix row fail before the first quality check.
- Workflow-policy fixtures continue rejecting tag/branch/short action references, omitted or non-attestation write permissions, persisted checkout credentials, and untrusted expressions reaching shell commands before Plan 0009 creates a hosted adapter.

- **Done when:** `scripts/quality` succeeds argument-free with verified pinned tools and one explicitly named exact-snapshot-only RustSec checkout, leaves no working-tree changes under cold and warm local harnesses, validates deterministic platform policy fixtures for every row, and runs real native commands only for the row matching the current environment while emitting an explicitly untrusted report.
