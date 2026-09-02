---
id: explicit-compensating-command
title: "Explicit Compensating Command"
workstream: "0033"
kind: task
depends_on:
  - successful-workflow-command
  - failed-workflow-command
gated: false
touches:
  - examples/finite-state-machine/compensation.machine.json
  - crates/slingshot-development/tests/fixtures/finite-state-machine-compensation/**
  - crates/slingshot-development/tests/finite_state_machine_compensation.rs
status: done
merged_as: "66f692dcf27360dd84e53db2dd4a886f478d9df8"
---
# Explicit Compensating Command

Prove compensation is an explicit machine effect with its own Slingshot operation rather than an inferred rollback.

**Steps:**

1. Commit the compensation machine, stable workflow-store namespace, exact compatibility identity and pre-effect drift cases, primary and downstream recursive-replication inputs, fake-author sequence including a downstream positive-prefix `admission_rejected`, expected three durable logical operations each with at most one fenced effect attempt and bounded no-op physical duplicates, positive and negative `ack.result.structured` authority-verification inputs including both recovery-evidence branches and terminal result-unavailable/authoritative-remote-success, separate approval input, and expected FSM histories before implementation.
2. Define a machine that successfully replicates primary content and then emits a distinct downstream recursive replication whose multiple-path manifest can prove a partial effect; route that handler's authority-neutral `on_failed` event to `failure_review_required` without emitting another effect or reserving a compensation logical operation.
3. In the development harness, read the failed effect's `ack.result.structured`, require absent `structured_sha256`, and compare it with the matching target-qualified daemon result and exact digest-bound logical-operation record. Send `authoritative_downstream_failure_verified` only when operation identifier, target digest, terminal state, registered `admission_rejected` category, positive accepted count, matching remaining count/current path, and `AuthoritativeRemoteFailure` disposition without certainty agree; that event enters `compensation_decision_required` and emits no effect or compensation logical operation.
4. Reject tool-local, JSON-RPC, authoritative-nonexecution, fail-closed-indeterminate, terminal `ResultUnavailable`/`AuthoritativeRemoteSuccess`, nonterminal `RecoveryRequired` with either `ExecutionCertainty` or `AuthoritativeRemoteSuccess`, zero-accepted or inconsistent replication failure, malformed conditional terminal payload, timeout, spawn, protocol, missing/digested structured value, mismatched-operation, and mismatched-target evidence without sending the verification event; keep the instance in review with no compensation operation.
5. Define a distinct `backup_restore_approved` domain-decision event as the only transition from `compensation_decision_required` that emits `restore_backup`; map that effect to `replicate_content` for a backup repository path with the compatibility manifest's sole nonempty operation-key suffix, exact literal `-backup-restore`.
6. Run the pinned real FSM executor, send the explicit approval only after verified authority, and require successful compensation before entering the compensated terminal state.
7. Compare exactly three fake-author logical operations, their at-most-one winning fences/effects and bounded no-op physical-record sets, daemon operation records, exact contract provenance, FSM acknowledgements, review/verification/approval events, and final history to fixtures.

**Tests:**

- Primary replication, partial-effect-capable downstream replication, and backup replication are the only three Slingshot logical operations and intended effects and occur in exact order; each admits at most one fenced command-effect attempt while any bounded duplicate physical records lose the gate and no-op.
- The three logical operations have distinct stable operation keys; primary and downstream keys use the empty suffix, while the approved backup key alone uses exact `-backup-restore` and still fits the 107-byte final-key bound. Physical Sling identifiers and record counts never establish effect identity.
- Every exhausted or immediate handler failure can reach only the authority-review state through `on_failed`; no such event directly emits compensation.
- The verification gate accepts only matching target-qualified registered positive-prefix replication evidence with `AuthoritativeRemoteFailure` and no certainty from `ack.result.structured`; it rejects local/tool, JSON-RPC, authoritative-nonexecution, fail-closed-indeterminate, both nonterminal recovery-evidence variants, terminal result-unavailable/authoritative-remote-success, zero-accepted/inconsistent, malformed, timeout, spawn, protocol, missing/digested, and mismatched evidence without reaching the decision state.
- Neither entry into `failure_review_required` nor `authoritative_downstream_failure_verified` creates a backup logical operation, physical record, or effect; only the later `backup_restore_approved` event emits `restore_backup` and admits its logical operation.
- The compensation handler's success event is the only transition into the compensated state.
- No implicit delete, rollback, publisher request, or hidden command appears in any trace.
- Every Slingshot command-line, Model Context Protocol, and daemon child uses the shared Plan 0004 typed private test-root bootstrap; the hostile temporary production-root sentinel is untouched and absent from output, and no production root override is introduced.
- Independently stale daemon-runtime or author-transport provenance fails before the first effect, and drift after an acknowledgement prevents authority verification/advance without fabricating a compensation operation.

- **Done when:** `SLINGSHOT_FINITE_STATE_MACHINE_EXECUTABLE=<finite-state-machine-executable> SLINGSHOT_EXECUTABLE=<slingshot-executable> cargo test -p slingshot-development --test finite_state_machine_compensation` proves only exact-provenance matching undigested `ack.result.structured` registered positive-prefix replication evidence with authoritative remote failure enters no-effect decision review, rejects both authoritative-remote-success forms and every other evidence fixture, and creates the third logical operation only after the separate `backup_restore_approved` event, with at most one fenced compensation effect despite bounded no-op physical duplicates, before reaching the compensated state.
