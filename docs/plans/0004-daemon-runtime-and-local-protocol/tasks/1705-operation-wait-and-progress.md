---
id: operation-wait-and-progress
title: "Operation Wait And Progress"
workstream: "0017"
kind: task
depends_on:
  - operation-status-and-result
gated: false
touches:
  - crates/slingshot-daemon/src/operation_wait.rs
  - crates/slingshot-daemon/tests/operation_wait.rs
status: done
merged_as: "4b2b75641e31c1ddb092988db287e0e9df33eee8"
---
# Operation Wait And Progress

Several local clients may observe one operation, and none may become an execution dependency. This task provides revision-based replay and bounded progress fan-out.

**Steps:**

1. Write deterministic concurrency tests first for immediate replay, two and maximum waiters, excess waiter, late waiter, stale revision, exact revision, target collision, slow/nonreading waiter, disconnected/cancelled waiter, retryable recovery, committed recovery resume, terminal replay, and daemon shutdown.
2. Register a waiter by author-target digest, operation identifier, and last observed revision, then immediately deliver any newer persisted state before subscribing to live progress.
3. Broadcast monotonically increasing persisted revisions through independent bounded waiter queues, coalescing superseded progress while preserving the latest recovery/resume and terminal update.
4. Remove disconnected, explicitly cancelled, or response-write-deadline-expired waiters without changing operation execution, repository state, or other waiters; write expiry closes that client transport and is observed as disconnect.
5. Return terminal state immediately to a waiter that starts after completion.
6. Keep an admitted wait attached until terminal state, explicit client cancellation, transport disconnect including a blocked-write deadline, or daemon shutdown; an attached wait with no inbound partial frame has no frame-read deadline, and the daemon never converts an application-level wait deadline into operation failure.

**Tests:**

- All waiters observe strictly increasing revisions and the same terminal fact.
- A late or stale waiter first receives the newest persisted revision rather than waiting for another event.
- A deliberately blocked waiter neither blocks execution nor causes another waiter's queue to grow without bound.
- Disconnect, cancellation, and shutdown release waiter resources deterministically.
- Injected response-write expiry detaches only the nonreading waiter, while advancing frame-read time against an otherwise attached wait does nothing.
- Waiter-count exhaustion returns bounded backpressure without changing the operation, and a same operation identifier in another target partition never receives an update.
- Advancing injected time alone neither detaches a waiter nor changes pending work into failed.

- **Done when:** `cargo test -p slingshot-daemon --test operation_wait` proves target-partitioned revision replay, terminal/recovery/resume delivery, bounded independent queues, transport-only slow-writer detachment, no read timeout for a valid attached wait, and nonblocking behavior at maximum waiter load, and all workspace gates succeed.
