---
id: sqlite-is-configured-before-the-first-connection
title: "SQLite Is Configured Before The First Connection"
workstream: "0070"
kind: task
depends_on: ["database-compatibility-precedes-mutation"]
gated: false
touches:
  - crates/slingshot-storage/src/database.rs
  - crates/slingshot-storage/src/lib.rs
  - crates/slingshot-storage/tests/migrations.rs
status: completed
merged_as: ""
---
# SQLite Is Configured Before The First Connection

The design requires a reviewed SQLite build with memory-only temporary storage and statement-journal spill disabled, but the implementation merely rejects `TEMP_STORE=0` after opening and never sets the global spill configuration.

**Steps:**

1. Introduce one process-wide SQLite initializer reached before any library call or connection and make a second, late, or partially failed initialization deterministic.
2. Bind the runtime library/source/version/compile-option identity and require exact `TEMP_STORE=3`; reject a merely nonzero or omitted option.
3. Apply and verify `SQLITE_CONFIG_STMTJRNL_SPILL=-1` before initialization, with no connection factory available until every configuration assertion holds.
4. Use process-isolated fixtures with wrong compile options, a prior accidental connection, and forced configuration failure; run real sort and statement-journal canaries without allowing disk spill.

- **Done when:** the exact reviewed SQLite identity, `TEMP_STORE=3`, and disabled statement-journal spill are established before the first connection in every product/test process, and any late or mismatched initialization refuses before database access.
