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
  - crates/slingshot-daemon/src/operation_scheduler.rs
  - crates/slingshot-daemon/tests/operation_scheduler.rs
  - "crates/slingshot-daemon/tests/fixtures/operation_scheduler/**"
status: done
merged_as: "38ccbbb4d883a9209639ec6cba8caadd3c534ce9"
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

- **Done when:** `cargo test -p slingshot-daemon --test operation_scheduler` matches every target/conditional-evidence/recovery fixture including explicit resume, proves bounded non-starving admission and clamped monotonic reconstruction under both wall-clock directions, and shows idempotency is timing-independent without a live retry loop, and all workspace gates succeed.
