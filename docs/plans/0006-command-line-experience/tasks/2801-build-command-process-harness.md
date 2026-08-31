---
id: build-command-process-harness
title: "Build Command Process Harness"
workstream: "0028"
kind: task
depends_on:
  - compose-command-line-application
gated: false
touches:
  - crates/slingshot-test-support/src/lib.rs
  - crates/slingshot-test-support/src/process_harness.rs
  - crates/slingshot-test-support/tests/process_harness.rs
  - "crates/slingshot-test-support/tests/fixtures/process-harness/**"
status: done
merged_as: ""
---
# Build Command Process Harness

Provide a product-independent child-process harness with isolated environment/filesystem roots, independent byte streams, deterministic signals, and instance-bound bounded cleanup.

**Steps:**

1. Commit helper-process fixtures for environment isolation, filesystem roots, responsive and unresponsive owned-child cleanup, stream backpressure, terminal modes, signal delivery, replacement after reap, process-identifier reuse, and deadlines.
2. Consume Plan 0001's platform capability contract for a retained instance-bound child/native process handle or supervision channel acquired at spawn and held through reap. Implement reusable bounded child ownership, stream capture, terminal emulation, signal injection through that retained instance primitive, explicit environment construction, and leak detection without depending on command-line, daemon, configuration, or development crates.
3. Expose a product-neutral cooperative-cleanup adapter. A product suite supplies Plan 0004's exact current-instance nonce and invokes `daemon.stop` for a responsive daemon; only an unresponsive child actually owned by the fixture may be terminated through its continuously retained handle/channel. Treat process identifiers as diagnostics only, never discover descendants by process identifier, and never check a process identifier and later signal it.
4. Prove a stale nonce or a handle for a reaped instance cannot stop a replacement even when its process identifier is reused, and prove the harness fails on orphaned owned children, nondrained streams, inherited environment state, and deadline overruns.

**Tests:**

- `process_harness` exercises every generic harness capability against deterministic helper processes, including a retained-handle unresponsive child, a cooperative current-nonce fake, stale nonce, replacement, and forced process-identifier reuse.
- Cleanup assertions prove no owned child, socket, or temporary runtime entry survives a case and a neighboring/replacement process is never signaled.
- A static/runtime sentinel fails if cleanup enumerates descendant process identifiers or performs a check-then-signal-by-process-identifier sequence.

- **Done when:** `cargo test -p slingshot-test-support --test process_harness` proves the reusable harness isolates environment and filesystem roots, contains every owned process and byte stream through cooperative current-nonce stop or a retained instance handle/channel, and cannot signal a replacement through a stale nonce, stale handle, or reused diagnostic process identifier.
