---
id: operation-lifecycle
title: "Operation Lifecycle"
workstream: "0015"
kind: task
depends_on:
  - daemon-runtime-contract
gated: false
touches:
  - crates/slingshot-domain/src/operation.rs
  - crates/slingshot-domain/tests/operation_lifecycle.rs
  - "crates/slingshot-domain/tests/fixtures/operation_lifecycle/**"
status: done
merged_as: ""
---
# Operation Lifecycle

Operation state is a durable business fact rather than an inference from process memory. This task defines the total lifecycle fold before SQLite or an executor can write it.

**Steps:**

1. Author transition-table fixtures first for queued, submitting, accepted, running, succeeded, and failed states, including duplicate, regressive, conflicting, and terminal cases.
2. Implement operation state/revision, bounded latest progress, result disposition, terminal failure kind/conditional disposition, execution certainty, conditional recovery execution evidence, and nonterminal retry/recovery value types with full-word fields and closed variants.
3. Define `OperationExecutionCertainty` as `ConfirmedNotExecuted`, `SubmissionUnknown`, or `RemoteOutcomeUnknown`. Define RecoveryExecutionEvidence as exactly `ExecutionCertainty` containing one of those values or `AuthoritativeRemoteSuccess` containing no certainty; define executor outcome as success, terminal failure, or nonterminal `RecoveryRequired` carrying that union.
4. Define terminal failure kinds `Rejected`, `RemoteFailed`, `ResultUnavailable`, `RecoveryWindowExpired`, `RemoteStateLost`, `IntegrityFailure`, and `RetryPolicyExhausted`; define the conditional disposition union as `AuthoritativeNonExecution` carrying exactly `ConfirmedNotExecuted`, `AuthoritativeRemoteFailure` carrying no certainty field, `AuthoritativeRemoteSuccess` carrying no certainty field, or `FailClosedIndeterminate` carrying exactly one unknown certainty. Permit RecoveryWindowExpired only for retired recovery truth before a remote terminal outcome is known; proven-success result/artifact retention expiry is only ResultUnavailable/AuthoritativeRemoteSuccess.
5. Define recovery scheduling as a closed bounded category including `PersistentCapacityUnavailable`, attempt count, retry observation UTC value, bounded delay, checked UTC diagnostic deadline, explicit manual-resume eligibility, and redacted detail. Require unresolved execution categories to carry ExecutionCertainty and post-success result/artifact/capacity categories to carry AuthoritativeRemoteSuccess. Reconstruct a checked monotonic deadline from injected startup clocks by clamping elapsed wall time to the original delay.
6. Implement a pure fold that increments revision once for a new lifecycle, progress, certainty, disposition, or recovery fact, treats identical duplicate as no-op, and rejects every unnamed fact.
7. Keep connection health, process identity, waiter state, and transport-specific errors outside lifecycle state.

**Tests:**

- Every allowed edge produces the fixture's next state and next revision.
- Every disallowed edge preserves the original value and returns the expected transition error.
- Succeeded and failed states are immutable under all later inputs.
- Duplicate facts are idempotent, while same-state facts with different terminal metadata conflict.
- Retryable or exhausted ambiguous outcomes preserve a nonterminal lifecycle state, survive serialization, and advance only conditional recovery-evidence/recovery/revision facts; proven remote success pending local completion uses AuthoritativeRemoteSuccess and no certainty.
- Rejected/definitely-not-executed exhaustion use authoritative nonexecution with confirmed nonexecution; remote failed requires authoritative remote-failure proof; result unavailable requires authoritative remote success; retired/lost/integrity use fail-closed indeterminate with retained uncertainty.
- Serialization rejects a certainty member on either authoritative remote disposition, a missing or non-confirmed certainty on authoritative nonexecution, a missing or confirmed certainty on fail-closed indeterminate, and every illegal recovery-category/evidence combination.
- Unknown-certainty retry exhaustion remains nonterminal, and the domain exposes no generic compensation-safety boolean.
- Forward/backward wall-clock restart vectors reconstruct a monotonic deadline between immediate eligibility and the original delay; exact timing never changes idempotency or lifecycle validity.
- Persistent-capacity unavailability is nonterminal, carries authoritative remote success without certainty, and cannot coexist with a published terminal result or allocate a replacement operation identity; authoritative post-success result loss becomes ResultUnavailable rather than unknown execution or RecoveryWindowExpired.
- Progress values at and below the named byte bound round-trip with their introducing revision; over-bound progress is rejected without mutation.

- **Done when:** `cargo test -p slingshot-domain --test operation_lifecycle` exhaustively checks every lifecycle/progress/certainty/result/recovery fact, the conditional kind/disposition payload matrix with no invented certainty, fail-closed uncertainty, absence of a compensation-safety claim, clamped restart clock, terminal immutability, and duplicate no-op, and all workspace gates succeed.
