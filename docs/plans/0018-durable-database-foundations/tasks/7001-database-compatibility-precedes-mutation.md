---
id: database-compatibility-precedes-mutation
title: "Database Compatibility Precedes Mutation"
workstream: "0070"
kind: task
depends_on: ["installation-state-is-one-locked-ledger"]
gated: false
touches:
  - crates/slingshot-storage/src/database.rs
  - crates/slingshot-storage/tests/migrations.rs
  - crates/slingshot-storage/tests/fixtures/migrations/tables.jsonl
status: planned
merged_as: ""
---
# Database Compatibility Precedes Mutation

The database is opened and connection settings are applied before migration code discovers a schema newer than this build. Even a refusal can create or alter sidecars and metadata, while the test checks only main-file length.

**Steps:**

1. Inspect an existing database through a verified no-follow, owner-private, single-link handle in a read-only/no-create/no-journal mode before applying any setting or migration.
2. Refuse an absent-but-not-creatable, nonordinary, linked, replaced, wide-permission/ACL, or future-schema database without modifying bytes, metadata, timestamps, sidecars, or directory entries.
3. Reopen only compatible state through the enforced factory, then perform creation or migration under the installation lock with explicit durable commit boundaries.
4. Compare complete directory inventory, file bytes, identities, permissions, and metadata before and after future-schema and injected-failure cases; include replacement between inspection and reopen.

- **Done when:** every incompatible or substituted database refusal leaves the entire pinned database directory byte-for-byte and entry-for-entry unchanged, and only an accepted identity can be created or migrated.
