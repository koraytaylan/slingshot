---
id: daemon-ping-service
title: "Daemon Ping Service"
workstream: "0003"
kind: task
depends_on:
  - minimal-local-protocol
  - daemon-runtime-ownership
gated: false
touches:
  - crates/slingshot-daemon/src/local_server.rs
  - crates/slingshot-daemon/src/service.rs
  - crates/slingshot-command-line/src/daemon_entry.rs
  - crates/slingshot-daemon/tests/ping_service.rs
status: planned
merged_as: ""
---
# Daemon Ping Service

The first daemon serves one method and publishes readiness only after the same endpoint can answer it.

**Steps:**

1. Build an in-process native-endpoint test that asserts bind, atomic readiness publication, ping, nonce-matched stop, stale-nonce refusal, bounded protocol failure, injected-deadline release, orderly shutdown, current-user isolation, and platform endpoint cleanup before implementing the server.
2. Implement the local accept loop under a held `DaemonOwnership`, with one bounded frame reader and one response writer per connection and the exact foundation-contract connection capacity. Apply only the manifest's initial-control-frame, incomplete-frame read-idle, absolute frame-completion, and response-write deadlines using an injected monotonic clock.
3. Dispatch retained `daemon.ping` and `daemon.stop`. Ping returns the request identifier, diagnostic process identifier, exact live readiness nonce, target names, and product version. Stop validates the supplied nonce against the held owner, sends its acknowledgement, then triggers orderly service shutdown; a stale nonce returns `stale_daemon_instance` and changes no state. Every other method returns method-not-found.
4. Add the internal daemon process entry that receives explicit runtime root, profile, and environment arguments, acquires ownership, binds, publishes readiness, serves, and maps startup failure to a typed process exit.

**Tests:**

- Readiness is absent before bind and atomically present only when ping succeeds.
- Concurrent connections receive correctly correlated responses with no frame interleaving.
- Malformed, oversized, unknown-version, and unknown-method requests leave the server available for the next valid ping.
- Connections with no initial frame, a partial length prefix, a declared-length partial payload, byte-drip beyond frame completion, or a blocked response writer close at the exact injected boundary and release capacity; a following valid ping succeeds.
- A fully read request followed by quiescence has no incomplete-frame deadline, so the transport rule does not treat an established idle connection as a partial frame.
- A second daemon entry for the same target exits without binding; another target binds concurrently.
- A current nonce acknowledges and performs orderly shutdown; a nonce from the prior owner cannot stop a replacement even under a reused-process-identifier fixture.
- Orderly shutdown removes its endpoint and matching readiness record while preserving the persistent lock file.

- **Done when:** `cargo test -p slingshot-daemon --test ping_service` passes over the current native Unix domain socket or Windows remote-rejecting named pipe, including exact manifest slow-client/write deadlines and capacity, concurrent clients, protocol faults, current-user/target isolation, atomic readiness, nonce-bound cooperative stop, stale-nonce refusal, and cleanup.
