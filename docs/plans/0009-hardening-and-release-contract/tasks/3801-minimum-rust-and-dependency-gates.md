---
id: minimum-rust-and-dependency-gates
title: "Minimum Rust And Dependency Gates"
workstream: "0038"
kind: chore
depends_on:
  - model-context-protocol-fuzzing
gated: false
touches:
  - rust-toolchain.toml
  - Cargo.toml
  - Cargo.lock
  - deny.toml
  - compatibility/rustsec-advisory-database.toml
  - scripts/quality
  - scripts/check_minimum_supported_rust_version
  - crates/slingshot-development/tests/toolchain_and_dependency_policy.rs
status: done
merged_as: "d654c9e52cf589525a62330788a0bdee493faea9"
---
# Minimum Rust And Dependency Gates

The workspace declares one minimum compiler and one locked dependency graph; release automation must exercise those declarations rather than silently using a newer compiler or resolving different packages.

**Steps:**

1. Write policy tests first for mismatched toolchain declarations, an unavailable minimum compiler, unlocked product or fuzz commands, unused dependencies, duplicate versions, unapproved licenses, advisories, registries, Git sources, feature drift, and absent, dirty, wrong-origin, wrong-full-commit, wrong-tree, ambient, or freshness-asserting explicitly named advisory checkouts.
2. Add `scripts/check_minimum_supported_rust_version`, reading the version from `[workspace.package].rust-version`, requiring the toolchain file to match, and running locked check, tests, linting, and rustdoc with that exact compiler.
3. Audit Plan 0001's retained exact-snapshot-only RustSec origin/full-commit/tree pin. Accept only `SLINGSHOT_RUSTSEC_ADVISORY_DATABASE_DIRECTORY`, verify exact detached clean bytes, and reject timestamp/age/freshness/review fields. Git author/committer times and ambient clocks cannot establish release freshness. Pin changes still use Plan 0001's explicit nonmutating proposal command; task `release-input-cache` separately requires a new owner-approved exact-pin record from the protected release run.
4. Tighten `deny.toml` and the dependency fixture expectations across `Cargo.lock` and `fuzz/Cargo.lock`, documenting each narrow duplicate or source exception as structured policy data.
5. Invoke the minimum-version and dependency checks from the argument-free `scripts/quality` without embedding another compiler-version literal; both the later owner-confirmed hosted adapter and release acceptance must prepare the exact pinned advisory checkout before the gate, verify it through the same repository-local command, and supply its explicit path through `SLINGSHOT_RUSTSEC_ADVISORY_DATABASE_DIRECTORY`. No quality command discovers, fetches, advances, repairs, or defaults an advisory checkout.
6. Confirm the commands leave the manifest, lockfile, and advisory checkout byte-identical.

**Tests:**

- The declared compiler builds and tests every workspace target and feature set.
- Each mismatched or weakened fixture fails with the responsible declaration or package.
- An absent, unverified, ambient, wrong-snapshot, or freshness-asserting advisory database cannot pass; arbitrary Git timestamps and wall-clock values leave the exact origin/commit/tree decision unchanged and never produce a release-freshness result.
- Locked resolution remains unchanged on cold and warm runs.
- The product and coverage-fuzzing graphs are both checked; a package reachable only from the fuzz lockfile cannot bypass license, source, or advisory policy.
- The hosted-adapter contract requires any later workflow to obtain the version from repository declarations, check out and verify the exact RustSec pin with credential persistence disabled in a network-enabled preparation step, export the one named advisory-directory input, and invoke these same local scripts without another fetch or fallback.

- **Done when:** `cargo test -p slingshot-development --test toolchain_and_dependency_policy`, `scripts/check_minimum_supported_rust_version`, and argument-free `scripts/quality` pass from the committed lockfile and explicitly named exact-snapshot-only Plan 0001 advisory checkout without modifying inputs or claiming current/release freshness.
