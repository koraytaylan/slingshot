# Plan 0019 — Operation State Atomicity

> Make receipts, results, subscription generations, capacity counters, and maintenance effects belong to exactly one durable state transition.

## Why this plan

The storage repositories model rich lifecycle invariants in Rust, but several checks occur before the transaction that commits their effect or persist only a summary label after the lifecycle has already committed. Recovery receipts are keyed without operation identity. Subscription events have no generation identity and byte admission ignores the incoming event. Maintenance revalidates before, not inside, its write transaction and reconstructs lossy replays; artifact cleanup never reaches its completed phase.

These are concurrency and crash-consistency gaps. Each happy path passes in a single-threaded test, but a second operation, reset response, lifecycle update, process fault, or restart can create states the public query layer already has to describe as missing or inconsistent.

## In scope

- **0071 — Recovery identity.** Resume receipts are scoped to one operation, and receipt lookup, exact lifecycle/revision/recovery eligibility, and insert/replay occur in one serialized compare-and-set transaction.
- **0072 — Successful settlement.** Lifecycle, canonical inline bytes or verified artifact associations, immutable result disposition, timestamp, and recovery clearing commit together with schema-level invariants and byte-identical reopen behavior.
- **0073 — Subscription generations and bytes.** Generation is part of every event identity and reset CAS. Incoming bytes are included in checked, transaction-local admission and reconciled against generation-scoped rows.
- **0074 — Maintenance application and cleanup.** Reviewed freshness, exact deletes, counts, and complete receipt commit together; a durable post-commit deletion journal advances to completed only after unreferenced physical artifacts are verified absent.

## Out of scope

Selecting which terminal records policy should retain is unchanged. Remote author semantics and scheduling are composed in Plan 0020. Artifact publication and aggregate reservation use Plan 0017; database enforcement uses Plan 0018. This plan does not promise exactly-once remote execution, but it does ensure one logical operation cannot replay another's receipt or expose a torn local result.

## Review evidence at `2808354`

- `operation_recovery.rs` derives a source fingerprint from command fingerprint, expected revision, and category but omits operation identity. The receipt table primary key and repository lookup use only target plus source. Validation reads the operation before a separate immediate transaction inserts/replays the receipt without re-reading lifecycle, revision, recovery, or manual eligibility. Tests use one operation constant and no barrier race.
- `operation_submission.rs` commits `Succeeded` and later calls `record_result_disposition` in a second transaction. It records only whether inline/artifact data existed, not the inline bytes or complete associations. `operation_queries.rs` explicitly errors on succeeded-plus-null disposition. Success preserves `outstanding_recovery`, and the standalone disposition mutator can rewrite rows without requiring succeeded/no-recovery state.
- `agent_subscription_ledger.rs::EventFact` carries no generation; event rows are keyed by target/subscription/cursor, while `install_high_water` performs an unconditional generation update. An old stream can append after reset, concurrent reset responses can regress the high water, and cursor reuse collides. Event capacity checks only whether existing bytes are already at the limit, then adds arbitrary incoming bytes without checked sum or per-event bound.
- `maintenance.rs` previews and compares a digest before opening its write transaction, then deletes by identifiers without revision/settlement predicates or affected-row equality. Replayed receipts report zero released rows because count and manifest are not persisted. Database artifact rows can be removed, but the physical release function has no caller and `ReceiptStage::Completed` has no producer.
