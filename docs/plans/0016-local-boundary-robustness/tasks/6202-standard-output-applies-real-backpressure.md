---
id: standard-output-applies-real-backpressure
title: "Standard Output Applies Real Backpressure"
workstream: "0062"
kind: task
depends_on: ["standard-input-is-bounded-while-read"]
gated: false
touches:
  - crates/slingshot-command-line/src/command_line.rs
  - crates/slingshot-command-line/src/model_context_protocol/application.rs
  - crates/slingshot-command-line/src/model_context_protocol/standard_stream_transport.rs
  - crates/slingshot-command-line/tests/model_context_protocol_transport.rs
  - crates/slingshot-command-line/tests/model_context_protocol_process_boundaries.rs
status: complete
merged_as: "pending"
---
# Standard Output Applies Real Backpressure

The application enqueues an answer, ignores queue refusal, marks it acknowledged, and also returns the line to a synchronous writer. The bounded queue and output deadlines are therefore a model exercised by unit tests, not the path that writes protocol stdout.

**Steps:**

1. Establish one output ownership path: reserve queue capacity before accepting work, enqueue each response exactly once, and let one writer drain complete lines in order.
2. Acknowledge only after the sink accepts the whole response. Propagate saturation and write/deadline failure into the active request lifecycle, detach waiters once, and stop reading when output cannot make progress.
3. Bound queued response count and bytes from the foundation contract and ensure notifications consume no response slot.
4. Run a compiled child with stdout deliberately undrained, fill to exact capacity and one beyond, and prove a bounded refusal or deadline exit without heap growth or hang; also prove ordered lossless output under slow progress and broken-pipe shutdown.

- **Done when:** every accepted response is written once before acknowledgement, a blocked or failed stdout cannot hang or grow the process beyond configured capacity, and real-process tests exercise the same queue/deadline state used in production.
