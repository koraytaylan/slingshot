---
id: event-stream-heartbeat
title: "Event Stream Heartbeat"
workstream: "0021"
kind: task
depends_on:
  - bounded-server-sent-event-decoder
gated: false
touches:
  - crates/slingshot-agent-connection/src/event_stream_heartbeat.rs
  - crates/slingshot-agent-connection/src/lib.rs
  - crates/slingshot-agent-connection/tests/fixtures/event-stream-heartbeat.jsonl
  - crates/slingshot-agent-connection/tests/event_stream_heartbeat.rs
status: done
merged_as: ""
---
# Event Stream Heartbeat

Turn stream activity into a deterministic liveness decision without turning a quiet connection into a failed remote job.

**Steps:**

1. Commit connect/header deadline, activity-before-deadline, activity-at-deadline, silence-past-deadline, comment-heartbeat, job-event, malformed-input, and clock-regression fixtures before implementation.
2. Implement EventStreamHeartbeat with HEARTBEAT_TIMEOUT and an injected monotonic clock.
3. Treat every complete comment or event as activity and expose Healthy or TimedOut connection state.
4. Use the shared connect/response-header deadlines for attachment, then make heartbeat timeout request reconnection only; do not impose a total stream-body deadline or create/mutate RemoteJobState.
5. Reject a regressing injected clock as a testable internal error rather than underflowing duration arithmetic.

**Tests:**

- Activity before and at the timeout boundary remains healthy.
- Silence immediately beyond the boundary yields one timeout signal.
- Comments and job events refresh liveness identically.
- Malformed partial bytes do not refresh activity until the decoder completes a valid comment or event.
- No heartbeat fixture or transition creates a Failed remote-job state.

- **Done when:** cargo test -p slingshot-agent-connection --test event_stream_heartbeat passes every boundary with an injected clock and proves timeout affects only connection state.
