---
id: idempotent-operation-repository
title: "Idempotent Operation Repository"
workstream: "0015"
kind: task
depends_on:
  - sqlite-schema-and-migrations
gated: false
touches:
  - crates/slingshot-storage/src/operation_repository.rs
  - crates/slingshot-storage/src/sqlite_statement_inventory.rs
  - crates/slingshot-storage/tests/operation_repository/admission.rs
  - crates/slingshot-storage/tests/operation_repository/fixtures.rs
  - crates/slingshot-storage/tests/operation_repository/lifecycle.rs
  - crates/slingshot-storage/tests/operation_repository/main.rs
  - crates/slingshot-storage/tests/operation_repository/recovery.rs
status: done
merged_as: "7b743b648d6f6eb83b7d80ae1a770bcb08b15272"
---
# Idempotent Operation Repository

This task turns the lifecycle and schema into atomic admission, transition, lookup, and recovery operations while keeping every idempotency decision in durable state.

**Steps:**

1. Write repository tests first for admission/replay/conflict, same identifier under two principal-bound targets including the same deployment/author with different opaque principals, atomic pre-executor InstallationIdentifier snapshot/crash, compare-and-set, bounded progress, every conditional recovery-evidence/category variant, multiple exact repeated/conflicting recovery-resume receipts, every conditional failure kind/disposition payload including ResultUnavailable/AuthoritativeRemoteSuccess, retry clock observation, ordered queues, old-target history, and reopen.
2. Key every repository method by author-target digest plus operation identifier and store the full identity, exact selected-environment revision, revision-bound fingerprint, and InstallationIdentifier snapshot in the same first-admission transaction as `queued`, before returning scheduler eligibility or permitting executor invocation.
3. Implement one `synchronous = FULL` transaction that inserts `queued` or returns the existing same-partition operation only when identifier, stored selected revision, and fingerprint match; acknowledge only after commit returns.
4. Return identifier conflict without mutation for another selected revision or fingerprint in the same partition, while admitting the same caller identifier independently in another target partition.
5. Implement revision-checked lifecycle, bounded progress, conditional recovery execution evidence, retry observation/delay/diagnostic, result disposition, and validated conditional terminal kind/disposition settlement plus point lookup and deterministic current-partition reconstruction.
6. Persist a bounded target-and-selected-revision-bound recovery-resume receipt for each committed source operation-revision/category/fingerprint in the same transaction as scheduler eligibility; query an exact receipt independently of the operation's later lifecycle but require its selected revision to match request and operation before replay or fresh compare-and-set validation.
7. Ensure every returned record is decoded through domain types rather than unchecked strings and expose the nonsecret target digest on summaries.

**Tests:**

- Concurrent identical target/revision/fingerprint admission creates one row and every caller receives that row; the same target/identifier under another selected revision conflicts unchanged.
- A crash at every first-admission boundary leaves either no row or one committed `queued` row containing the exact InstallationIdentifier snapshot; no executor or remote boundary can observe an admitted row without it.
- Concurrent conflicting admission yields one winner and deterministic conflicts without overwriting content.
- A stale expected revision cannot write; the current expected revision increments exactly once.
- Reopening reconstructs nonterminal operations in caller and enqueue order, while terminal operations remain queryable and unscheduled.
- Replay never crosses author-target partitions, including partitions differing only by opaque authentication principal; terminal old-target rows remain queryable and the same operation identifier may exist independently in each partition.
- Recovery-required outcomes survive reopen with exact conditional evidence/scheduling; post-success completion cannot decode as unknown certainty, and terminal rows decode exactly one legal conditional disposition payload without persisting a generic compensation-safety claim.
- Each recovery-resume source selected-revision/operation-revision/category/fingerprint commits one bounded receipt, identical concurrency replays that receipt, later lifecycle/recovery/terminal revisions do not hide it, and every wrong-revision, stale, or conflicting fresh precondition preserves the row.

- **Done when:** `cargo test -p slingshot-storage --test operation_repository` proves full-commit admission with its immutable pre-executor InstallationIdentifier snapshot, exact target/selected-revision/fingerprint replay or conflict, conditional terminal dispositions, revision-checked lifecycle/progress/recovery, selected-revision-bound durable resume-receipt replay after later and terminal revisions, deterministic reconstruction, and atomic result settlement across reopen, and all workspace gates succeed.
