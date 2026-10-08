---
id: fair-bounded-operation-scheduler
title: "Fair Bounded Operation Scheduler"
workstream: "0017"
kind: task
depends_on:
  - idempotent-operation-repository
  - operation-executor-boundary
gated: false
touches:
  - crates/slingshot-storage/tests/producer_fairness.rs
status: done
merged_as: "06ddf691464062838c44b7db11d31c3695b8dbc6"
---
# Fair Bounded Operation Scheduler

Capacity limits must reject overload predictably, and a busy caller must not starve another. This task implements scheduling as a pure decision over durable queue facts.

**Steps:**

1. Author schedule fixtures first for empty/capacity/fairness/order, recovery not eligible/at boundary/explicitly resumed, confirmed-not-executed and ambiguous certainty, authoritative-remote-success pending completion, selected target, live monotonic observation, restart, and forward/backward UTC clock changes.
2. Read the exact global-pending, global-in-flight, per-caller-pending, and per-tick-selection bounds only from the typed `DaemonRuntimeContract`; construction refuses a missing or mismatched digest and defines no local default.
3. Implement transactional admission accounting and a pure round-robin selector that preserves enqueue order within each caller.
4. Derive all order from persisted target digest, caller, enqueue sequence, conditional recovery evidence, and recovery facts. In-process eligibility uses the monotonic deadline or a committed explicit-resume eligibility fact; restart derives a new checked monotonic deadline by clamping injected UTC elapsed time between zero and the original delay and preserves an explicit resume.
5. Keep clocks and retry/reconciliation time as explicit inputs and output directives only; this plan does not run a live retry timer or advance remote work, and ordering/idempotency do not depend on exact clock timing.
6. Return capacity exhaustion before a partial operation is inserted.

**Tests:**

- Every fixture produces the exact ordered admission or start decisions.
- A continuously busy caller cannot prevent another admitted caller from selection.
- No decision exceeds any bound, including after slots are released concurrently.
- A fresh scheduler given the same repository observation produces byte-identical decisions.
- Rows in another target partition and recovery rows before eligibility never enter current directives; one committed resume becomes eligible without allocating work, while backward UTC movement cannot extend beyond original delay and forward movement cannot create duplicate work.


## Production replacement

The original pure selector and its model-only fixtures were retired during the
2026-09-29 effectiveness review. Production now orders active producer turns in
SQLite and rotates only after the atomic claim succeeds. The runtime applies
monotonic retry eligibility to that ordered snapshot. Admission validates the
opaque producer identity and applies per-producer and global pending limits in
the same transaction that resolves operation replay.

Current behavioral checks: storage `producer_fairness`, daemon library
`operation_dispatch::tests::producers`, and `runtime_builder::retry_schedule::tests`.
These cover durable ordering, reopen, contention, idle/rejoining producers,
overflow rollback, capacity isolation, immutable replay ownership, and monotonic
readiness. They replace the former independent caller-sorted model.
