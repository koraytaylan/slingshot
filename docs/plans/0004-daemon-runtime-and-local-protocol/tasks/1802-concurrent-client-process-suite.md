---
id: concurrent-client-process-suite
title: "Concurrent Client Process Suite"
workstream: "0018"
kind: task
depends_on:
  - operation-restart-recovery
  - automatic-daemon-start
  - graceful-daemon-stop
gated: false
touches:
  - crates/slingshot-test-support/src/process_barrier.rs
  - crates/slingshot-development/tests/concurrent_daemon_clients.rs
status: done
merged_as: ""
---
# Concurrent Client Process Suite

The singleton claim must hold during the startup race users actually create: many short-lived processes discovering an absent daemon at the same instant.

**Steps:**

1. Use Plan 0001's path-only process harness, private stable-child supervisor, and barrier with the exact walking-client count, deadlines, scheduling tolerance, and server connection capacity read through the embedded typed `FoundationContract`; release product clients against an absent target and separate helper clients against the existing development binary's internal test-daemon subcommand. Do not restate those values in Plan-0004 source or fixtures, and do not add another executable target.
2. Have product clients complete identity-bearing hello and receive unavailable execution with no operation rows.
3. Have helper clients submit the same target-partitioned identifier, wait to terminal, paginate list operations, and report daemon nonce, target digest, revision, and result.
4. Fill the local connection ceiling with a cohort split across no hello, partial length prefix, partial payload, byte drip, and nonreading response; drive named monotonic boundaries and prove capacity is released for later valid hello and operation clients.
5. Repeat with distinct identifiers/callers for fairness/backpressure and with two distinct targets racing as the first global installation users to prove one installation identifier and ledger.
6. Retain terminal history, change one namespace's author target, and prove independent same-identifier replay in each target partition. Then deliberately make a supervised owner unresponsive, terminate-and-wait only through the supervisor's retained instance-bound child/native handle, and prove one fresh-nonce successor after audits. Never look up, check, or signal by the diagnostic process identifier.
7. On responsive success/failure cleanup, send `daemon.stop` with each daemon's exact current nonce and wait through the manifest cooperative-stop deadline. Use the retained supervisor handle only for deliberate/unresponsive fallback, wait through the manifest supervision deadline, prove every owned child is reaped before temporary-root removal, and verify stale nonce/supervision tokens cannot affect the successor even when a fixture reuses its process identifier. Print process identifiers only for diagnostic correlation with target, barrier, deadline, and global-lock evidence when a watchdog fires.

**Tests:**

- The exact manifest client cohort observes one daemon nonce/owner and no admitted product row; helper clients observe one fake execution and one terminal result.
- Fair scheduling services every admitted caller; overload responses match the configured capacity and can retry.
- A second target remains independent, and exactly one successor recovers the killed first target while all new clients join it.
- Distinct-target first starts serialize through one global installation-state lock and retain one installation identifier with both ledger registrations.
- Same operation identifiers in distinct author-target partitions never collide, and all list/status output names the correct target digest.
- Every slow-client variant releases its slot at the exact boundary, and a subsequent valid client completes without killing a long-lived wait that has no incomplete frame.
- Cooperative cleanup uses only the current nonce, unresponsive cleanup uses only the retained stable handle, all owned children are reaped, and stale nonce/token plus reused-process-identifier fixtures cannot stop or signal the replacement.

- **Done when:** `cargo test -p slingshot-development --test concurrent_daemon_clients` passes product-unavailable, helper-idempotency, slow-client-capacity-release, global-first-start, fairness/backpressure, target-partition, and owner-successor process scenarios at the exact manifest cohort/bounds, with nonce-bound or retained-handle cleanup and no process-identifier signalling, and all workspace gates succeed.
