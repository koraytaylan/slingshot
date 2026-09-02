---
id: execute-operation-service
title: "Execute Operation Service"
workstream: "0017"
kind: task
depends_on:
  - fair-bounded-operation-scheduler
  - persistent-capacity-accounting
gated: false
touches:
  - crates/slingshot-daemon/src/operation_submission.rs
  - crates/slingshot-development/Cargo.toml
  - crates/slingshot-daemon/tests/operation_submission.rs
  - crates/slingshot-development/tests/operation_submission_process.rs
status: done
merged_as: "70d16a7ff0cba081c5c7b795c4aa3ab57f499d41"
---
# Execute Operation Service

Execute admission is the boundary where caller idempotency, durable queueing, scheduling, executor invocation, artifacts, and terminal settlement meet.

**Steps:**

1. Write daemon tests first for unavailable execution, daemon-runtime-contract-digest and target/revision mismatch, scheduler and logical/physical persistent-storage refusal, and no-row guarantees; write helper fixtures for admission/replay/conflict, success, every failure kind/disposition combination including ResultUnavailable/AuthoritativeRemoteSuccess, all recovery evidence/category combinations including exhausted unknowns and completed-result capacity unavailability, result dispositions, artifacts, and concurrent identical callers.
2. Verify expected author identity and selected-environment revision before repository access. If the product's executor is unavailable, return the stable unavailable response before fingerprinting or admission.
3. In the helper composition, fingerprint and atomically admit plus capacity-account `(AuthorTargetIdentityDigest, OperationIdentifier)` in `queued` before acknowledging execute; an exact replay consumes no new row and exhaustion returns maintenance guidance without insertion.
4. Let the scheduler select durable work, commit `submitting`, invoke once per attempt, and persist accepted, running, bounded progress, recovery, and terminal facts through revision-checked repository methods.
5. Leave timeout, cancellation, connection loss, indeterminate submission, unknown remote outcome, explicit retryability, and exhausted unknown retries nonterminal as `RecoveryRequired` with ExecutionCertainty/scheduling. Set `failed` only with a validated authoritative or fail-closed disposition; fail-closed loss/integrity retains unknown certainty.
6. Validate a successful logical result, keep it inline within the machine budget or reserve/install daemon-created canonical `application/json` in `structured_result`, reserve/install declared artifacts, then atomically commit result disposition and terminal state. Capacity refusal after proven remote completion commits only `PersistentCapacityUnavailable` with AuthoritativeRemoteSuccess and never resubmits remote work or publishes partial success; authoritative result/artifact loss settles ResultUnavailable with that same remote-success disposition.

**Tests:**

- Product execute returns unavailable and creates no operation; target/revision mismatch reaches neither repository nor executor.
- Concurrent identical helper requests in one target partition create one operation and invoke the fake once; the same identifier in another partition is independent.
- Repeating a terminal operation returns it without execution; conflicting content never mutates it.
- Capacity refusal inserts no operation and permits the same request to succeed after capacity is available.
- A required completed-result artifact that cannot reserve capacity leaves the same operation nonterminal with maintenance guidance, exact recovery facts, no result slot, and no second remote invocation; capacity release plus exact resume finishes installation.
- Recovery-required unknowns remain nonterminal with exact certainty/scheduling after exhaustion; post-success recovery carries authoritative remote success without certainty. Failed outcomes expose kind plus exactly one legal conditional disposition payload, never invent certainty for either authoritative remote disposition, and never claim domain-specific compensation safety.
- Inline or structured-result/artifact metadata and terminal success appear atomically; installation failure cannot expose success or an unchecked URL.

- **Done when:** `cargo test -p slingshot-development --test operation_submission_process` plus the focused daemon test prove no-row unavailable/mismatch refusal, full-commit target-partitioned admission, one helper invocation per attempt, nonterminal retry facts, and atomic inline-or-artifact settlement, and all workspace gates succeed.
