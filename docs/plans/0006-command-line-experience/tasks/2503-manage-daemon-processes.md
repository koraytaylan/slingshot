---
id: manage-daemon-processes
title: "Manage Daemon Processes"
workstream: "0025"
kind: task
depends_on:
  - resolve-command-targets
gated: false
touches:
  - crates/slingshot-command-line/src/daemon_process.rs
  - crates/slingshot-command-line/tests/daemon_process.rs
  - crates/slingshot-test-support/src/daemon_process.rs
status: done
merged_as: ""
---
# Manage Daemon Processes

Start, inspect, ping, or stop exactly one daemon for the selected namespace and complete every lifecycle exchange without shell evaluation or implicit remote cancellation.

**Steps:**

1. Build scripted process fixtures for existing, absent, concurrently starting, incompatible, unhealthy, exact/stale/malformed `DaemonRuntimeContractDigest`, author-target-mismatched, selected-environment-revision-mismatched, early-exit, timeout, ready, status, ping, nonce-mismatched stop, stale-nonce owner replacement, process-identifier reuse, graceful stop, and already-stopped daemons.
2. Recompute the exact digest of canonical `policy/daemon-runtime-contract-1.json`, format `slingshot.daemon-runtime-contract/1`, against its repository/embedded bytes and sidecar. Implement endpoint probing, explicit start, exact executable spawning, detached streams, namespace-lock convergence, readiness polling, exact `DaemonRuntimeContractDigest`, current `AuthorTargetIdentity`, and `SelectedEnvironmentRevision` handshake validation, and child-failure capture with named bounds. Treat process identifiers as diagnostics only; retain the instance-bound spawn/supervision handle needed by the owning process harness rather than discovering or later signaling an owner by process identifier.
3. Implement status and ping without spawning, and stop through the exact current-nonce local protocol request followed by bounded ownership-release observation; a stale nonce cannot stop a replacement, and none of these leaves opens remote transport.
4. Run barrier-controlled races and assert one serving process and one normal local-protocol connection per namespace.

**Tests:**

- `daemon_process` covers each scripted lifecycle and exact failure class, including runtime-digest and target/revision mismatch diagnostics with explicit update or stop/start guidance. Runtime mismatch blocks versioned operation access but not retained ping/status/current-nonce stop.
- The concurrent-start case proves all clients converge on one daemon process.
- Status, ping, and stop against an absent namespace cause zero child spawns; current-nonce stop against a live owner releases the endpoint and leaves persistent operation state intact, while stale-nonce and reused-process-identifier cases leave the replacement untouched.

- **Done when:** `cargo test -p slingshot-command-line --test daemon_process` proves concurrent invocations create at most one exact-runtime-contract correctly targeted ready daemon per namespace, stale owners are refused with update/stop-start guidance while retained control remains usable, and status/ping/stop never spawn or contact Adobe Experience Manager.
