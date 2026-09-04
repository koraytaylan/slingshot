---
id: maintenance-application-revalidates-in-its-transaction
title: "Maintenance Application Revalidates In Its Transaction"
workstream: "0074"
kind: task
depends_on: ["event-capacity-includes-the-incoming-event"]
gated: false
touches:
  - crates/slingshot-storage/src/maintenance.rs
  - crates/slingshot-storage/migrations/**
  - crates/slingshot-daemon/tests/operation_maintenance.rs
status: completed
merged_as: ""
---
# Maintenance Application Revalidates In Its Transaction

Maintenance compares a preview digest before its write transaction, then deletes identifiers without the reviewed revision/settlement predicates and ignores affected-row counts. Replay reconstructs zero released rows because the exact result was never persisted.

**Steps:**

1. In one immediate transaction, find or replay the receipt, re-read every reviewed row, compare exact revision/lifecycle/result/retention predicates and manifest digest, and refuse any drift before deletion.
2. Delete with the same predicates, require exact affected-row counts, update all logical capacity counters, and persist the complete immutable manifest, released counts, artifact candidates, and database-applied stage in the receipt.
3. Make concurrent apply attempts converge on one winner and return byte-identical receipt/result on replay before and after restart.
4. Place barriers after preview and before delete, change each reviewed field, race two applies, and inject statement/commit faults; assert no stale or partial deletion.

- **Done when:** maintenance deletes exactly the still-reviewed rows or none, every replay returns the original complete result across restart, and no concurrent lifecycle/revision change can be hidden by an `Applied` receipt.
