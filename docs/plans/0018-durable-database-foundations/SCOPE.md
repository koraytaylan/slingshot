# Plan 0018 — Durable Database Foundations

> Make installation identity and SQLite policy runtime-enforced, race-safe, and byte-preserving before the product daemon adopts them.

## Why this plan

The persistence design documents strong properties: one stable installation identity, no-follow owner-private files, serialized replacement, a restrictive SQLite file surface, exact no-spill configuration, closed SQL, and unchanged refusal of a future schema. The implementations currently expose those properties as helper objects, source inventories, or tests while opening ordinary paths and raw database connections directly.

This code is presently reached by tests and development harnesses rather than `run_daemon_entry`. That is not a reason to lower severity; it is the reason this plan precedes product composition. Wiring it first would turn same-user path races, silent scan errors, pre-refusal database mutations, unconstrained SQL/VFS access, and unmodeled side files into the durable product boundary.

## In scope

- **0069 — Installation identity ledger.** One verified directory handle and global lock serialize scan/read/classify/replace. Record and stage files are no-follow, bounded, owner-private, single-link objects, and publication is durable and crash-safe on Unix and Windows.
- **0070 — Enforced SQLite boundary.** Compatibility is inspected without mutation before accepted open; database roots and files are verified and pinned; exact build/runtime no-spill settings, file-open policy, SQL/PRAGMA inventory, and physical high-water behavior are enforced by production connections rather than exposed as optional helpers.

## Out of scope

Schema changes for operation semantics belong to Plan 0019. Artifact blob files belong to Plan 0017. This plan does not promise a bespoke VFS if an equivalently enforceable directory-handle and SQLite-authorizer architecture proves every required property; it does require the final mechanism to mediate real SQLite opens and statements. Product startup consumes this boundary only in Plan 0020.

## Review evidence at `2808354`

- `installation_state.rs` follows links with `File::open`, reads without a byte cap, accepts any regular file on Windows, omits link/ACL/stable-identity checks, uses a fixed `.staging` path opened with create-plus-truncate, and discards directory-entry errors through `flatten`. Its declared lock path has no production caller, so first-start and replacement sequences are not serialized. Tests cover ordinary classification/read/replace, not links, permissions, scan errors, processes, or crash boundaries.
- `database.rs` calls `rusqlite::Connection::open(path)` directly and exposes the raw connection publicly. It applies connection settings before migration compatibility rejects a future schema. `sqlite_vfs.rs` explicitly says its whitelist is not a registered or delegating VFS and is used only in tests. No product source sets `SQLITE_CONFIG_STMTJRNL_SPILL=-1`; compile-option validation merely rejects `TEMP_STORE=0` rather than requiring the specified `TEMP_STORE=3`.
- The SQL inventory and object whitelist are source/policy assertions, not enforcement against `ATTACH`, unlisted PRAGMAs, transient/journal objects, or future callers using the raw connection. The future-schema test compares only main-file length, so it does not prove byte, metadata, directory-entry, or sidecar preservation. Physical WAL/database limits are modeled but not placed around every production write/checkpoint path.
