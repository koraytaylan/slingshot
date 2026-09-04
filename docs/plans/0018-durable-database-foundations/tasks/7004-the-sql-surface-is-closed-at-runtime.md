---
id: the-sql-surface-is-closed-at-runtime
title: "The SQL Surface Is Closed At Runtime"
workstream: "0070"
kind: task
depends_on: ["sqlite-file-policy-mediates-real-opens"]
gated: false
touches:
  - crates/slingshot-storage/src/database.rs
  - crates/slingshot-storage/src/sqlite_statement_inventory.rs
  - crates/slingshot-storage/src/lib.rs
  - crates/slingshot-storage/tests/migrations.rs
status: planned
merged_as: ""
---
# The SQL Surface Is Closed At Runtime

The reviewed statement inventory is source data, while `Database` publicly exposes `&rusqlite::Connection`. Any current or future consumer can execute arbitrary SQL, PRAGMAs, attachments, or extensions outside the inventory.

**Steps:**

1. Keep raw connections and transaction primitives private to the storage implementation and expose only reviewed typed repository operations.
2. Install a runtime SQLite authorizer on every connection that admits the exact statement/action/PRAGMA set required by those operations and denies attachment, extension loading, schema mutation outside migrations, and transaction escapes.
3. Bind every prepared statement to an inventory identity and fail tests/build when typed code introduces SQL or a PRAGMA without an explicit reviewed entry.
4. Attempt each prohibited action through public and internal canary callers and mutate the inventory independently; prove refusal occurs before side effect and no alternate raw handle can be obtained.

- **Done when:** every executed SQLite action belongs to the reviewed typed inventory, prohibited SQL and PRAGMAs are denied at runtime before effect, and the storage public API exposes no raw connection or bypass.
