---
id: stream-progress-and-detach-cancellation
title: "Stream Progress And Detach Cancellation"
workstream: "0031"
kind: task
depends_on:
  - execute-operation-tools
gated: false
touches:
  - crates/slingshot-command-line/src/model_context_protocol/progress_and_cancellation.rs
  - crates/slingshot-command-line/tests/model_context_protocol_progress.rs
status: done
merged_as: ""
---
# Stream Progress And Detach Cancellation

Translate daemon state into correlated monotonic progress and make cancellation, end of input, or output-failure shutdown detach only local waiters and later messages.

**Steps:**

1. Commit event schedules for progress, heartbeats, local daemon reconnect/replacement, terminal completion before response write, complete response write acknowledgement, duplicate identifier before/after release, cancellation before/after acceptance, end of input, queue-pressure/write failure, active-registry saturation, detach-all, and cancellation/terminal/write-acknowledgement/end-of-input/output-failure races in both revisions.
2. Implement progress-token correlation, monotonic filtering, per-request response suppression/detachment, writer-acknowledged completion release, idempotent detach-all on output failure, and race resolution by durable receipt/state sequence plus Plan 0029's registry and once-only shutdown transitions.
3. Assert cancellation and output failure send no daemon or agent cancellation; after cancellation release only after suppression/detachment, and after output failure every active waiter/progress token is removed, every reservation releases exactly once, and no producer can enqueue a later response or notification.

**Tests:**

- `model_context_protocol_progress` pins exact notification sequences for every schedule and revision.
- Local replacement continues one request without synthetic failure; cancellation/end-of-input detach their applicable waiters, output failure detaches all waiters, and every durable operation remains readable after a new client connects.
- Race cases prove terminal completion cannot enqueue after fail-stop wins, queued-but-unwritten completion remains reserved, cancellation releases only after suppression/detachment, identifier reuse cannot overtake release, and a concurrently removed waiter/reservation is finalized exactly once.

- **Done when:** `cargo test -p slingshot-command-line --test model_context_protocol_progress` proves daemon replacement preserves one monotonic attached stream while cancellation/end-of-input/output failure detach the exact local waiter set, release each active identifier only at its defined linearization point, enqueue no post-failure message, and never cancel or mutate durable work.
