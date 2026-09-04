---
id: recovery-receipts-belong-to-one-operation
title: "Recovery Receipts Belong To One Operation"
workstream: "0071"
kind: task
depends_on: []
gated: false
touches:
  - crates/slingshot-daemon/src/operation_recovery.rs
  - crates/slingshot-daemon/src/operation_submission.rs
  - crates/slingshot-storage/src/operation_repository.rs
  - crates/slingshot-storage/migrations/**
  - crates/slingshot-daemon/tests/operation_recovery.rs
status: planned
merged_as: ""
---
# Recovery Receipts Belong To One Operation

Two operation identifiers with the same command, target, expected revision, and recovery category currently derive the same target-wide receipt key. The second can replay the first receipt, while precondition checks can go stale before insert.

**Steps:**

1. Add operation identity to the canonical recovery receipt key, schema uniqueness, lookup, and serialized representation while retaining deterministic exact-repeat replay for that operation.
2. In one immediate transaction, read the named operation, compare lifecycle/revision/recovery/manual eligibility, and insert or replay only a receipt whose stored operation and source match.
3. Use an explicit expected state/version so a concurrent terminal, progress, revision, or recovery update has exactly one winner and a stale resume cannot commit `Applied`.
4. Test operations A and B with identical command fingerprints independently, exact repeat across restart, forged cross-operation lookup, and barriers against every eligibility-changing transition.

- **Done when:** no operation can observe or replay another operation's recovery receipt, exact same-operation replay remains stable across restart, and eligibility plus receipt commit has one transactionally consistent winner.
