---
id: automatic-daemon-start
title: "Automatic Daemon Start"
workstream: "0016"
kind: task
depends_on:
  - daemon-local-server
gated: false
touches:
  - crates/slingshot-command-line/src/daemon_connection.rs
  - crates/slingshot-command-line/src/daemon_process.rs
  - crates/slingshot-command-line/tests/automatic_daemon_start.rs
status: done
merged_as: "4c317d6830a460409ad622239ab8cc3ed806fc3a"
---
# Automatic Daemon Start

A command-line or Model Context Protocol process must connect to an existing target daemon or start exactly one without leaking daemon output into its own protocol streams.

**Steps:**

1. Write child-process tests first for matching existing owner, target mismatch, revision mismatch, absent owner, simultaneous contenders, spawn failure, readiness timeout, incompatible operation version, early child exit, responsive cleanup, deliberately unresponsive cleanup, reused diagnostic process identifier, stale nonce/supervision token after replacement, and distinct name pairs.
2. Load the client's selected environment, implement connect-and-control-hello before any spawn attempt, and compare expected `AuthorTargetIdentity` plus `SelectedEnvironmentRevision` with hello.
3. On absence, contend for the startup guard; let only its winner spawn the detached daemon while other clients wait and retry using the exact total-start and maximum-retry-delay values from the embedded typed `FoundationContract`, then connect. Do not copy either value into connector code or fixtures.
4. On target or revision mismatch, refuse with explicit guidance to inspect and explicitly stop/restart the named owner; never route, join, kill, signal, or stop it automatically, and never treat its process identifier as authority.
5. Spawn the same absolute product executable through a dedicated daemon entrypoint with only profile/environment names plus test-only root overrides, isolate all standard streams, and route diagnostics to its owned sink. Production detachment intentionally outlives the starter. The test-only spawn adapter must register the exact spawned instance with Plan 0001's private supervisor, which retains the unreaped child/native process handle until one atomic exit observation or terminate-and-wait disposition; no cleanup path may check a process identifier and later signal it.
6. For responsive test cleanup, call retained `daemon.stop` with that instance's exact hello/readiness nonce and wait through the `FoundationContract` cooperative-stop deadline. For a deliberately unresponsive child, address only its instance-bound supervisor token and retained stable handle and wait through the manifest supervision deadline. After replacement, prove the old nonce/token has no effect even when a fixture reuses the diagnostic process identifier.
7. Return structured startup stage and cause information without exposing credentials or internal command payloads.

**Tests:**

- An existing compatible owner causes no child spawn.
- A mismatched target or selected-environment revision is never joined or stopped and returns exact explicit stop/restart guidance.
- Simultaneous contenders for one namespace observe one instance nonce and one spawned daemon.
- Distinct targets start distinct daemons without contention.
- Spawn, readiness, early-exit, and protocol failures are distinguishable, bounded, and free of protocol-output contamination.
- Success and induced-failure teardown reap every owned client and cooperatively stop or stable-handle-terminate-and-wait every supervised daemon before removing temporary roots; stale nonce/token and reused-process-identifier fixtures cannot redirect cleanup to a replacement.

- **Done when:** `cargo test -p slingshot-command-line --test automatic_daemon_start` proves one detached product owner under concurrent absence, exact target/revision matching, manifest-bounded start behavior, nonmutating mismatch guidance, independent namespaces, isolated client protocol streams, and current-nonce cooperative or retained-stable-handle cleanup with no process-identifier signalling or stale-instance impact, and all workspace gates succeed.
