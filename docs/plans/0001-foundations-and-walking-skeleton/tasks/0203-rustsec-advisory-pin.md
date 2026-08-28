---
id: rustsec-advisory-pin
title: "RustSec Advisory Pin"
workstream: "0002"
kind: task
depends_on:
  - source-policy-checker
gated: false
touches:
  - compatibility/rustsec-advisory-database.toml
  - crates/slingshot-development/src/rustsec_advisory_pin.rs
  - crates/slingshot-development/src/main.rs
  - crates/slingshot-development/tests/rustsec_advisory_pin.rs
  - "crates/slingshot-development/tests/fixtures/rustsec-advisory-pin/**"
status: planned
merged_as: ""
---
# RustSec Advisory Pin

The foundation gate authenticates one exact advisory-database snapshot. Git authors control commit timestamps, so this task makes no current-freshness, review-time, or release-time claim from repository metadata.

**Steps:**

1. Author correct, wrong-origin, wrong-full-commit, wrong-tree, dirty, missing-object, shallow, ambient-cache-only, and reviewed-update fixtures before the verifier.
2. Commit `compatibility/rustsec-advisory-database.toml` with exactly the canonical RustSec repository URL, one full commit identifier, and one canonical content-tree digest. The schema rejects a branch, tag, short identifier, timestamp, age, `fresh` flag, review assertion, or mutable location because none authenticates current release freshness.
3. Verify only the directory explicitly named by `SLINGSHOT_RUSTSEC_ADVISORY_DATABASE_DIRECTORY`: exact normalized origin, detached full commit, clean index/worktree, no untracked path, and canonical tree digest must match the pin. Never discover an ambient cache, fetch, repair, advance, or accept a positional checkout during a quality run.
4. Extend the exhaustive development-binary dispatcher with an explicit `rustsec-pin-review` command that preserves its existing branches, rejects unknown commands, accepts one deliberately named candidate directory, verifies its origin/full commit/clean tree/digest, and emits proposed pin bytes for human review without fetching, modifying either repository, committing, reading a wall clock, or asserting freshness.
5. Label every result `exact_snapshot_only`. Plan 0009's owner-gated release preparation separately binds an owner-reviewed pin subject to an authenticated external integrated time and compares that time with authenticated release-artifact times; Plan 0001 neither creates nor accepts that stronger evidence.

**Tests:**

- The schema rejects every timestamp, age, freshness, or review field, and arbitrary Git author/committer timestamps and wall-clock changes cannot alter exact origin/commit/tree acceptance.
- Wrong origin/commit/tree, dirty checkout, shallow/missing object, unset directory, and ambient-cache-only fixtures fail before advisory analysis.
- The review command emits complete proposed origin/full-commit/tree data only for a verified explicit candidate, never changes candidate or workspace bytes, and never labels the proposal current or fresh.
- A fixture copied years later still proves only the same exact snapshot; no Plan 0001 output can satisfy Plan 0009's owner-review or authenticated release-time freshness fields.

- **Done when:** `cargo test -p slingshot-development --test rustsec_advisory_pin` proves exact origin/full-commit/clean-tree authentication, explicit nonmutating pin-proposal output, rejection of every timestamp/freshness assertion, and zero ambient-clock/cache/fetch dependence without making a current-release claim.
