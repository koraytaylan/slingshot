---
id: list-operations
title: "List Operations"
workstream: "0017"
kind: task
depends_on:
  - idempotent-operation-repository
  - local-operation-envelopes
  - operation-status-and-result
gated: false
touches:
  - "crates/slingshot-daemon/tests/fixtures/list-operations/**"
  - crates/slingshot-daemon/src/operation_queries.rs
  - crates/slingshot-daemon/tests/list_operations.rs
  - crates/slingshot-storage/src/operation/listing.rs
  - crates/slingshot-storage/src/operation/mod.rs
  - crates/slingshot-storage/src/operation_repository.rs
  - crates/slingshot-storage/src/sqlite_statement_inventory.rs
status: done
merged_as: "b8e22f149de4d1b3a1e544985d5b0ed2550cdb57"
---
# List Operations

Command-line and workflow clients need bounded discovery of durable work without scanning command payloads or guessing operation identifiers.

**Steps:**

1. Author empty/single/exact/continuation/concurrent-insert, every normalized filter, terminal old-target, wrong-target/filter/order/version, bit-tampered/stale cursor, and maximum-bound fixtures first.
2. Add the target-digest, descending-enqueue-sequence, and operation-identifier index and query it through a bounded repository method.
3. Define a versioned opaque cursor binding exact target digest, canonical normalized filter-set digest, descending enqueue ordering identifier, last enqueue sequence/operation identifier, and integrity digest; reject malformed, tampered, wrong-target/filter/order/version cursors before repository scanning.
4. Return bounded summaries containing target digest, identifiers, state/revision, conditional recovery execution evidence, terminal failure kind/disposition including authoritative remote success where applicable, latest progress/recovery/resume facts, timestamps, workflow correlation, and artifacts but no payload/reference/path/secret or generic compensation-safety claim.
5. Make target partition mandatory and support bounded lifecycle, terminality, caller, and workflow filters. Use stable enqueue sequence plus operation identifier, never timestamps, for order and continuation.

**Tests:**

- Every page stays within the named limit and concatenated pages equal the stable repository order without duplicates or omissions.
- Concurrent insertion does not corrupt a cursor or reorder/duplicate/omit rows relative to its stable sequence position.
- Terminal old-target rows remain listable by target digest, while default current-target listing cannot cross partitions.
- Malformed, tampered, wrong-target/filter/order/version, and over-bound requests fail before row materialization and summaries contain no forbidden values.

- **Done when:** `cargo test -p slingshot-daemon --test list_operations` proves index-backed bounded pagination, stable cursor continuation, target partitioning, every filter, and payload/secret exclusion against committed fixtures, and all workspace gates succeed.
