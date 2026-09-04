# Plan 0017 — Artifact Publication and Capacity

> Make every artifact publication non-overwriting, substitution-resistant, durable, and charged to one namespace-wide capacity authority before product composition can expose it.

## Why this plan

Artifact code handles untrusted sizes and user-chosen destinations, yet its final filesystem transitions use check-then-rename or trust an existing digest-named path without re-verifying it. Predictable sidecars can follow links, parent directories are not held by verified handles, and directory synchronization errors are discarded even though success claims crash durability. The aggregate-capacity object is an in-memory helper that production install and completion paths never call; separately constructed accounts do not share reservations.

The artifact store and completion path are not yet reachable through the shipped stub daemon. That limits current exposure but raises the bar for Plan 0020: unsafe dormant primitives must not become the production persistence boundary merely because they already compile.

## In scope

- **0067 — Atomic filesystem publication.** CLI downloads use a platform-correct no-replace primitive and private, no-follow staging/lock objects. Store publication pins verified directories, verifies an existing digest object before deduplicating, propagates durability failures, and reconciles partials.
- **0068 — Namespace capacity.** One persisted authority reserves aggregate bytes before streaming, converts reservations atomically to verified committed blobs and associations, charges deduplication once, and recovers abandoned reservations across processes and restarts.

## Out of scope

Remote artifact acquisition, retention policy, package-manager distribution, and maintenance selection are unchanged. Plan 0019 owns transactionally complete operation results and deletion journals; Plan 0020 owns product reachability. This plan does not claim filesystem primitives can defend against a fully compromised same-user process after it gains arbitrary code execution, but it does refuse same-user races and substitutions at every checked boundary.

## Review evidence at `2808354`

- `artifact_download.rs::publish` calls `symlink_metadata`, then `std::fs::rename`; Unix rename replaces a destination created in that gap. `artifact_staging_lock.rs` opens a predictable path with generic create/read/write and no create-new, no-follow, owner/mode/link-count, or retained-identity check. The test covers only a destination present before the check.
- `artifact_store.rs::install` returns success when a digest-named destination exists without verifying those existing bytes and then uses a replacing rename for the remaining race. Store setup uses `create_dir_all`; final-component no-follow does not pin parents; validation omits link/ACL checks; `synchronize_directory` converts every `sync_all` failure to success; failed streams can leave partials. Existing tests cover serial deduplication and final mode, not corrupt preexistence, ancestor replacement, directory-sync failure, or competing publishers.
- `persistent_capacity.rs` keeps reservations in a mutex owned by one object. `reserve_artifact` has no production caller, `ArtifactStore::install` enforces only the per-artifact maximum, `operation_repository` constructs fresh accounts, and `artifact_completion.rs` declares an `ArtifactSink` with no implementation. Aggregate committed-plus-reserved limits can therefore be exceeded serially, concurrently, or after independent construction if these paths are wired as they stand.
