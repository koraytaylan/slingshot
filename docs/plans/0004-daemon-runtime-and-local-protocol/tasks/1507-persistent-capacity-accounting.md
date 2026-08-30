---
id: persistent-capacity-accounting
title: "Persistent Capacity Accounting"
workstream: "0015"
kind: task
depends_on:
  - checksum-verified-artifact-store
  - idempotent-operation-repository
gated: false
touches:
  - "crates/slingshot-storage/tests/fixtures/persistent-capacity/**"
  - crates/slingshot-domain/src/operation.rs
  - crates/slingshot-domain/src/persistent_capacity.rs
  - crates/slingshot-storage/src/artifact_store.rs
  - crates/slingshot-storage/src/operation_repository.rs
  - crates/slingshot-storage/src/persistent_capacity.rs
  - crates/slingshot-storage/src/sqlite_statement_inventory.rs
  - crates/slingshot-storage/tests/persistent_capacity.rs
status: planned
merged_as: ""
---
# Persistent Capacity Accounting

Named conservative namespace limits keep retained operations and artifacts from consuming storage without bound while preserving every nonterminal fact and requiring explicit maintenance for deletion.

**Steps:**

1. Author empty, one-below/exact/one-above operation, recovery-resume receipt, maintenance-application receipt, the derived 257 target maintenance-result associations, artifact, database, write-ahead-log header/frame/transaction growth, shared-memory, write-transaction, reader-lifetime, checkpoint, replacement-space, exact object whitelist, duplicate-content, concurrent admission/reservation, pinned-reader/write-churn, interrupted install, restart, maintenance release, and filesystem-refusal fixtures before accounting code.
2. Consume the typed `DaemonRuntimeContract` as the sole source of maximum retained operations, receipts and target maintenance-result associations, committed-plus-reserved and individual artifact bytes, filesystem reserve, database/WAL/SHM/transaction/page/checkpoint/reader/deadline bounds, header/frame values, and formulas. Independently recompute the association limit as one current preview plus preview/application results for every retained application receipt, and recompute WAL maximum, maximum transaction WAL growth, high-water, active aggregate, replacement aggregate, and reserve from primitive operands. Check that `MAXIMUM_INDIVIDUAL_ARTIFACT_BYTES` equals the manifest's checked maximum of every Plan 0003 remote-slot maximum and the daemon canonical-result/maintenance-document maximum; reject an unrepresentable registry entry or inconsistent manifest rather than defining a local value.
3. Persist namespace counters and exact artifact reservations transactionally. New-operation admission increments its row count in the same transaction; exact replay consumes no new capacity. Recovery resume and maintenance apply reserve their bounded receipt and maintenance-result association rows atomically with the represented effect, while exact receipt/result replay consumes no capacity. Artifact reservation uses expected length and counts committed unique content plus active reservations, while verified duplicate content consumes no second blob allocation.
4. Refuse a new operation, fresh receipt, artifact reservation, or write transaction before partial mutation when a manifest logical limit, database page limit, WAL backpressure threshold, maximum transaction frame growth, exact whitelisted-object aggregate, physical replacement reserve, or checked filesystem safety reserve is unavailable. Interrupt internal readers at the exact lifetime, checkpoint within its deadline, and return typed bounded usage/limit facts plus explicit maintenance/retry guidance; never infer physical capacity from row counts, account an unlisted transient as free, or delete terminal rows, receipts, or committed artifacts automatically.
5. Associate a fully synchronized verified artifact and convert its reservation to committed usage in one crash-safe transaction. Abort/interruption releases only uncommitted reservation/staging state after reconciliation and never changes a terminal fact or referenced blob.
6. Define the manually resumable `PersistentCapacityUnavailable` recovery category with `AuthoritativeRemoteSuccess` recovery evidence and return the typed refusal needed for an execution service to leave completed remote work nonterminal without publishing a result slot or inventing execution uncertainty.

**Tests:**

- Every registry-declared remote slot and maximum local structured/maintenance result fits MAXIMUM_INDIVIDUAL_ARTIFACT_BYTES; exact operation/receipt/257-association/artifact limits succeed, and the first value above any limit refuses before row, reservation, temporary file, association, or slot creation and reports bounded maintenance guidance.
- Concurrent contenders cannot overcommit any counter, and exact operation/receipt replay or verified duplicate content consumes no second capacity unit.
- Every crash boundary reconstructs counters and reservations from authoritative rows/blobs without leak or double count.
- Independent arithmetic vectors prove the 32-byte WAL header, every 24-byte-header/page frame, one-maximum-transaction growth, high-water, active aggregate, replacement aggregate, and equal reserve. Pinned readers and maximum write churn cannot grow WAL or SHM beyond those values; reaching backpressure refuses later writes before mutation, bounded readers are interrupted, a completed checkpoint reopens admission, and restart validates/checkpoints only under the exact recovery policy.
- Manual terminal maintenance releases exact operation/resume-receipt/selected-prior-application-receipt/maintenance-result-association/reference/blob capacity only after its transactional rules commit and reserves the new apply receipt/results atomically; superseding the sole current preview follows its explicit association rule and no age/pressure path deletes retained state.
- Capacity refusal returns the exact manually resumable domain recovery category without changing operation identity/fingerprint or creating an artifact/result fact.

- **Done when:** `cargo test -p slingshot-storage --test persistent_capacity` proves manifest-derived logical bounds and independently recomputed header/frame-aware SQLite/database/WAL/SHM/transaction/reader/checkpoint/replacement/reserve bounds over the closed physical-object set, pinned-reader backpressure, concurrency and restart accounting, manual-only release, replay-without-recharge, and typed no-partial-publication refusal when result capacity is unavailable, and all workspace gates succeed.
