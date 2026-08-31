---
id: daemon-local-server
title: "Daemon Local Server"
workstream: "0016"
kind: task
depends_on:
  - stable-local-control-protocol
  - single-daemon-ownership
  - daemon-configuration-startup
  - secure-daemon-diagnostics
gated: false
touches:
  - crates/slingshot-daemon/src/local_server.rs
  - crates/slingshot-daemon/tests/local_server.rs
status: done
merged_as: ""
---
# Daemon Local Server

This task turns ownership into a ready local server while keeping request semantics behind a test handler until the operation service lands.

**Steps:**

1. Author endpoint integration tests first for pre-readiness audit order, identity-bearing hello, operation incompatibility with retained control, simultaneous connections, no-hello/partial-prefix/partial-payload/byte-drip/nonreading-response peers, malformed frames, manifest connection/deadline bounds, Windows named-pipe construction with and without the required remote-client rejection flag, abrupt clients, and current-nonce shutdown cleanup.
2. Bind the current platform's typed Plan 0001 endpoint only after ownership, selected-environment snapshot, installation comparison, database migration, and cross-partition nonterminal audit succeed. Every Windows named-pipe server creation path must use the platform adapter with the exact external `PIPE_REJECT_REMOTE_CLIENTS` value required by `FoundationContract` in addition to current-user access control; a raw or alternate constructor without that flag is unrepresentable or refused.
3. Publish readiness atomically after the listener can answer hello with instance nonce, selected author target, selected-environment revision, exact `DaemonRuntimeContractDigest`, stable control version, and supported operation versions; never publish a half-started daemon.
4. Read the connection capacity, frame bound, initial-control-frame, incomplete-frame read-idle, absolute frame-completion, and response-write deadlines directly from the embedded typed `FoundationContract`, enforce them from an injected monotonic clock, and prohibit Plan-0004 defaults or duplicated test values. Isolate each connection and dispatch stable control independently of a separately injected versioned-operation handler.
5. On shutdown, close the listener and remove only endpoint and readiness records carrying the current instance nonce.

**Tests:**

- A client cannot observe ready before configuration, installation, target audit, database access, and hello succeed.
- Missing/mismatched installation state and old-target nonterminal rows yield no bind or readiness and remain byte-for-byte unchanged.
- An operation-incompatible client still completes hello, ping, daemon status, and nonce-protected stop while compatible clients continue.
- Malformed and abruptly closed clients do not panic or affect later connections.
- The connection bound refuses excess clients deterministically and releases capacity when a client exits.
- A ceiling-sized slow-client cohort closes at exact injected boundaries and releases capacity for a later valid hello; a complete hello followed by no partial input is not subject to the incomplete-frame deadline.
- A nonreading response peer closes at the write deadline without changing daemon state or another connection.
- Closed platform-policy fixtures cover every abstract row and reject any Windows constructor missing `PIPE_REJECT_REMOTE_CLIENTS`. When the current native row is Windows, the native endpoint check also proves current-user local access and remote-client refusal when that probe is available; an unavailable remote probe is recorded as untrusted/not-run and creates no aggregate or release claim.

- **Done when:** `cargo test -p slingshot-daemon --test local_server` proves fail-closed pre-bind ordering, identity-bearing atomic readiness, exact manifest-driven slow-peer deadline release for a later valid client, retained control under operation incompatibility, bounded simultaneous service, mandatory Windows `PIPE_REJECT_REMOTE_CLIENTS`, and nonce-safe cleanup through deterministic row fixtures plus at most the one matching current-native endpoint, and all workspace gates succeed.
