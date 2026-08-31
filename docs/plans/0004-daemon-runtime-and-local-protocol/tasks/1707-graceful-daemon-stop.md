---
id: graceful-daemon-stop
title: "Graceful Daemon Stop"
workstream: "0017"
kind: task
depends_on:
  - local-request-dispatch
gated: false
touches:
  - crates/slingshot-daemon/src/shutdown.rs
  - crates/slingshot-daemon/src/request_dispatch.rs
  - crates/slingshot-daemon/tests/shutdown.rs
status: done
merged_as: ""
---
# Graceful Daemon Stop

Stop must release process ownership without converting unfinished durable work into a state the operation lifecycle does not contain.

**Steps:**

1. Write shutdown tests first for idle, queued, submitting, running, terminal, active waiter, new connection during stop, manifest cooperative-stop deadline exhaustion, repeated stop, wrong/prior-instance nonce, replacement, and reused diagnostic process identifier.
2. Validate retained `daemon.stop` against the exact current hello/readiness nonce without requiring operation-protocol compatibility. Namespace/target context and process identifiers are not authorization; return `stale_daemon_instance` before any transition for every stale nonce.
3. Acknowledge the accepted current-instance transition once, stop new admission and listener acceptance, notify waiters of server shutdown, and allow already completed executor callbacks to commit only within the remaining cooperative-stop budget read from `FoundationContract`; do not define a second top-level stop/grace deadline.
4. Within that budget, run the daemon-owned bounded checkpoint, synchronize database and artifact state, leave unfinished operations nonterminal, and remove only current-owner endpoint/readiness records.
5. Make repeated or stale-nonce stop requests deterministic and harmless. A stale nonce must not stop admission, close the listener, remove readiness, signal a process, or otherwise affect a replacement even if a process identifier is reused.

**Tests:**

- Idle and terminal-only daemons stop cleanly and release ownership.
- Queued, submitting, and running operations remain recoverable rather than becoming terminal or disappearing.
- New admission is refused after stop begins, while an already completed outcome can commit within grace.
- Wrong/stale-nonce and repeated stop requests return the exact retained result and cannot stop a successor or delete its endpoint; process identifiers are never signalled.

- **Done when:** `cargo test -p slingshot-daemon --test shutdown` proves operation-version-independent current-nonce `daemon.stop`, manifest-bounded checkpoint/settlement, recoverable nonterminal state, waiter release, and current-instance endpoint/readiness cleanup while stale nonces and reused diagnostic process identifiers cannot affect a replacement, and all workspace gates succeed.
