---
id: frame-standard-stream-messages
title: "Frame Standard Stream Messages"
workstream: "0029"
kind: task
depends_on:
  - model-context-protocol-module-scaffold
gated: false
touches:
  - crates/slingshot-command-line/src/model_context_protocol/active_request_registry.rs
  - crates/slingshot-command-line/src/model_context_protocol/protocol_diagnostics.rs
  - crates/slingshot-command-line/src/model_context_protocol/standard_stream_transport.rs
  - crates/slingshot-command-line/tests/model_context_protocol_transport.rs
  - crates/slingshot-test-support/fixtures/model-context-protocol/transport/**
status: done
merged_as: ""
---
# Frame Standard Stream Messages

Implement bounded newline-delimited JSON-RPC intake, active-request ownership, drop-capable diagnostics, and one canonical stdout writer with deterministic queue-pressure, write-deadline, and fail-stop behavior.

**Steps:**

1. Commit byte fixtures for requests, notifications, identifiers, the exact preference-ordered supported-revision JSON value `["2026-07-28","2025-06-18"]`, duplicate active identifiers, released-identifier reuse, active-count saturation by silent attached requests, duplicate keys, malformed envelopes, invalid directions, encoding failures, line/depth limits, concurrent writes, the maximum maintenance-resource read response, slow/closed output, queue count/byte saturation, full-queue producer timeout, write timeout before bytes, sink failure after a prefix, full/closed/blocked stderr, end-of-input drain, and shutdown/cancellation/response-release races.
2. Implement typed envelope parsing, `ProtocolRevision`, the one closed ordered supported-revision authority, and `ActiveRequestRegistry` with a named maximum. Make both era handlers borrow the exact same two-item authority without a local copy or sorting. Atomically reserve before handler creation; reject duplicate active identifiers without dispatching/replacing/detaching the original, and reject distinct over-limit requests without dispatch through one bounded resource-limit response.
3. Retain a reservation after response enqueue and release it only on the sole writer's complete-line acknowledgement; release cancelled requests only after response suppression plus waiter/progress detachment commit, and release end-of-input/output-failure entries only after detach-all cleanup. Permit identifier reuse only after that point.
4. Implement named message/depth bounds, canonical complete-line serialization before enqueue, a single-owner queue bounded by named message/byte limits, and named queue-pressure/write/output-failure-shutdown deadlines.
5. Implement one idempotent output-failure transition for pressure expiry, write expiry, and closed/failed stdout: stop intake, reject producers, discard unstarted queued lines, notify the application to detach all local waiters, release reservations after suppression/detachment, write no subsequent output, and finish cleanup boundedly.
6. Implement `ProtocolDiagnosticSink` with named message/byte bounds and zero-wait producer enqueue. Drop and saturating-count records when stderr is full/closed, never synchronously write from ordinary/fail-stop/panic paths, and never wait for or join a blocked diagnostic writer.
7. Pin partial-sink behavior: zero accepted stdout bytes emits nothing; a sink-accepted prefix followed by failure closes stdout without newline or later bytes, making only the terminal unterminated suffix invalid while every completed line remains parseable.
8. Stress concurrent response/notification producers and race queue/write failure with registry saturation, duplicate identifiers, cancellation, end of input, producer admission, writer acknowledgement, and ordinary shutdown.

**Tests:**

- `model_context_protocol_transport` pins every accepted/rejected input, exact complete output line, active-request transition, named deadline decision, queue transition, dropped-diagnostic fact, and terminal partial-sink prefix.
- A fixture proves the shared revision authority contains exactly `["2026-07-28","2025-06-18"]` and exposes one typed borrowed API; the later era/transcript task tests consume that API and reject an independently declared or differently ordered list.
- Duplicate identifiers never dispatch or disturb the original; silent requests saturate exactly at the named limit; overload responses are bounded; a response queued but not fully written remains active; reuse succeeds only after writer acknowledgement or completed cancellation cleanup.
- Cancellation/terminal/write-acknowledgement/end-of-input/output-failure races release each reservation exactly once, never exceed the active bound, and never dispatch a saturated request.
- Concurrency cases prove no complete line interleaves, queue count/bytes never exceed their constants, one failure transition wins, no line begins afterward, and shutdown completes within the injected deadline.
- Slow/closed/full stdout cases stop intake and request detach-all without durable cancellation; slow/closed/full stderr drops diagnostics and cannot delay a valid following request or bounded exit.

- **Done when:** `cargo test -p slingshot-command-line --test model_context_protocol_transport` proves the sole exact ordered two-revision authority, bounded active-request admission with duplicate/reuse linearization, bounded queue/write fail-stop including the largest maintenance-resource response, drop-capable nonblocking diagnostics, complete stdout lines plus at most one terminal prefix, exact once-only detach/release, and no remote cancellation.
