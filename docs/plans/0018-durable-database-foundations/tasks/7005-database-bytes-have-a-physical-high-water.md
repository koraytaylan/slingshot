---
id: database-bytes-have-a-physical-high-water
title: "Database Bytes Have A Physical High Water"
workstream: "0070"
kind: task
depends_on: ["the-sql-surface-is-closed-at-runtime"]
gated: false
touches:
  - crates/slingshot-storage/src/database.rs
  - crates/slingshot-storage/src/persistent_capacity.rs
  - crates/slingshot-storage/src/operation_repository.rs
  - crates/slingshot-storage/src/agent_job_repository.rs
  - crates/slingshot-storage/tests/persistent_capacity/**
  - crates/slingshot-storage/tests/migrations.rs
status: planned
merged_as: ""
---
# Database Bytes Have A Physical High Water

Logical row accounting and pure formulas do not constrain the bytes SQLite can hold in its main file and sidecars during real transactions, checkpoints, or recovery.

**Steps:**

1. Measure and reserve physical growth for the complete permitted object set before write transactions, with checked arithmetic and one namespace-shared authority.
2. Bound WAL growth and checkpoint scheduling under concurrent readers/writers; refuse before crossing the high water and preserve a recoverable prior transaction.
3. Reconcile logical counters, database pages, WAL, shared memory, and reservations after commit failure, process crash, and restart; fail closed on an unexplained object or byte total.
4. Exercise exact/plus-one physical bounds, long readers preventing checkpoint, concurrent writers, injected full-disk/sync failures, crash recovery, and reopen. Assert peak directory bytes as well as final logical rows.

- **Done when:** the complete SQLite object set never exceeds its declared physical high water during transaction or recovery, refusals retain a usable prior state, and restart reconciles every byte and reservation before serving work.
