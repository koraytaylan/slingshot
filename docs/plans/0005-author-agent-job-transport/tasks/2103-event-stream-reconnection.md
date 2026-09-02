---
id: event-stream-reconnection
title: "Event Stream Reconnection"
workstream: "0021"
kind: task
depends_on:
  - event-stream-heartbeat
gated: false
touches:
  - crates/slingshot-agent-connection/src/event_stream_reconnection.rs
  - crates/slingshot-agent-connection/src/lib.rs
  - crates/slingshot-agent-connection/tests/fixtures/event-stream-reconnection.jsonl
  - crates/slingshot-agent-connection/tests/event_stream_reconnection.rs
status: done
merged_as: "aaf3bab4899c550d71c605f3f419238665045c15"
---
# Event Stream Reconnection

Reconnect one lost filtered author stream with its durable generation and Last-Event-ID, injected bounded jitter, and explicit cursor-reset recovery while preserving every job's independent sequence and state.

**Steps:**

1. Commit first-connect, exact canonical subscription/generation query, wrong/duplicate/missing/surplus query, uncommitted/wrong-generation Last-Event-ID, filtered-subscription, clean-close, heartbeat-timeout, connect/header deadline, HTTP/1.1/HTTP/2 equivalence, unsupported protocol/upgrade/migration, informational head, declared trailer before stream exposure, undeclared empty/nonempty actual trailer after valid events, ambiguous framing/invalid HTTP/2 header semantics/over-bound encoded-or-decoded head, 408/429/500/502/503/504, bounded `Retry-After`, transport-error, deterministic jitter samples, increasing/capped backoff, restart with unchanged/forward/backward UTC, successful connection reset, cursor-expired, generation-changed, equal-cursor conflict, interleaved-job cursor, and without-cursor fixtures before implementation.
2. Implement capped exponential full jitter from named initial, multiplier, and maximum constants using injected monotonic clock and random source; persist attempt, EventReconnect category, chosen remaining delay, and diagnostic UTC eligibility instant.
3. Construct only the fixed event route with exactly the canonically ordered `daemon_subscription_identifier` and `agent_event_store_generation` query members from persisted target-partition facts. Send Last-Event-ID only after the corresponding subscription-ledger cursor/digest has committed; do not condition cursor persistence on a job foreign key or state transition and never accept a server-provided route.
4. Reset backoff only after a validated HTTP/1.1-or-HTTP/2 trailer-undeclared connection, route EventStreamResetRequired to snapshot/high-water reset recovery, treat an actual trailer as protocol-loss reconnect without a cursor fact of its own, and on equal-cursor digest conflict leave the cursor unchanged, record an integrity incident, enter Degraded, and require full-subscription high-water/snapshot reset before streaming resumes.
5. On restart reconstruct a monotonic deadline by clamping diagnostic-UTC residual from zero through the persisted delay; keep filtered-stream health and wall-clock movement separate from RemoteJobState, identities, generation, and certainty.

**Tests:**

- The exact delay/cap sequence follows deterministic random samples without sleeping; distinct samples remain within the named full-jitter interval.
- A successful connection resets the next delay to the named initial value.
- The fixed route has exactly the persisted subscription/generation query pair; Last-Event-ID is absent initially and equals the latest durably applied subscription cursor even when job-local sequences interleave, repeat, or have no associated local job.
- A received but unpersisted subscription cursor is never sent after simulated termination.
- Cursor-expired and changed-generation responses never invent a cursor advance and invoke the explicit reset algorithm.
- Forward/backward UTC jumps produce only the clamped due-now/original-delay schedule and cannot change a durable operation fact.
- Disconnect/reconnect leave every remote-job state unchanged until reducer or snapshot input, and restart preserves the clamped selected delay semantics.
- Informational, declared-trailer, framing, compressed/decoded-head, and protocol-version failures expose no stream; an undeclared actual trailer reconnects without retracting independently committed valid event/cursor facts or manufacturing a new one.

- **Done when:** cargo test -p slingshot-agent-connection --test event_stream_reconnection passes deterministic injected-jitter/restart clocks, durable retry, filtered generation/cursor, reset routing, full-subscription conflict degradation/reset, and state-separation cases.
