---
id: platform-runtime-contract
title: "Platform Runtime Contract"
workstream: "0001"
kind: spike
depends_on:
  - minimal-local-protocol
  - supported-platform-matrix
  - workspace-capability-probes
gated: false
touches:
  - crates/slingshot-daemon/src/platform_runtime/**
  - crates/slingshot-command-line/src/platform_runtime/**
  - crates/slingshot-test-support/src/platform_runtime/**
  - crates/slingshot-development/src/platform_runtime_contract.rs
  - crates/slingshot-development/tests/platform_runtime_contract.rs
  - "crates/slingshot-development/tests/fixtures/platform-runtime/**"
  - support/platform-runtime-evidence.schema.json
status: planned
merged_as: ""
---
# Platform Runtime Contract

Fix the platform policy and current-environment native process/local-endpoint behavior before daemon ownership and startup rely on operating-system assumptions. Plan 0001 never aggregates reports from machines it does not own; Plan 0009 maps all abstract rows to exact owner-approved native environments and authenticates their evidence.

**Steps:**

1. Commit exact-target, wrong-host, cross-compile-only, unsupported-target, every `FoundationContract` endpoint/deadline boundary, current-user-isolation, Windows remote-client, separate owner/startup-election lock contention, elected-client crash, atomic-readiness, stale-owner, detached-process, stable-supervision, exited-child, and process-identifier-reuse fixtures before platform modules.
2. Define one behavioral interface for endpoint naming, listener and connection creation, daemon-lifetime owner locking, client-lifetime startup-election locking, readiness publication, stale recovery, current-user access, detached child creation, and current-row reporting without exposing authority through a process identifier. The two lock identities and handle types are distinct and cannot be substituted or transferred. The test-support adapter additionally exposes a private supervision channel whose owner retains the exact spawned `std::process::Child`/native process handle unreaped until one atomic terminate-and-wait disposition; it never implements check-identity-then-signal-by-process-identifier.
3. Implement Unix domain socket and separate advisory owner/startup-election lock modules for supported Linux and macOS targets, including owner-only runtime directories, same-directory atomic readiness replacement, stale socket cleanup only under the owner lock, operating-system release of an election lock after client death, and session-independent detached child creation.
4. Implement the Windows named-pipe, separate exclusive owner/startup-election lock, current-user access-control-list, atomic readiness, election-owner death release, and detached-process equivalents. Every server creation call includes the exact external `PIPE_REJECT_REMOTE_CLIENTS` flag required by `support/foundation-contract.toml`; a closed Windows API fixture rejects any constructor path without it, and named-pipe lifetime follows live server handles rather than Unix socket unlink rules.
5. Consume the validated abstract `support/platforms.toml` rows and `support/foundation-contract.toml` limits. Deterministic policy fakes exercise every row, but a real invocation runs only the single row that exactly matches the current environment's target, host, and architecture. A nonmatching row, cross-compile result, family label, or a collection of copied reports cannot become Plan 0001 support evidence.
6. On a current native Windows environment, run the real local/remote pipe probe when the explicit remote-client fixture capability is available and require local-current-user success plus remote-client refusal; otherwise record the remote connection case as `not_run_untrusted` without weakening the mandatory creation-flag fixture. Plan 0009 must provide and authenticate the real Windows remote-client case before release support.
7. Emit at most one canonical schema-validated runtime report containing the source revision, abstract row digest, current observed environment facts, foundation-contract digest, and each real-versus-policy outcome. Label it `untrusted_current_native_observation`; it is never aggregate authority. Plan 0009 reruns the commands on owner-mapped rows and trusts a report only after its digest and exact environment are provider-attested.

**Tests:**

- Pure contract fixtures produce identical ownership/readiness decisions on every supported target while platform-specific endpoint values remain typed and separate.
- A matching native Linux or macOS current environment uses a real Unix domain socket/lock; a matching native Windows current environment uses a real named pipe/Windows lock whose constructor includes `PIPE_REJECT_REMOTE_CLIENTS`.
- The one current-native job proves current-user endpoint isolation, atomic readiness, one daemon owner under owner-lock contention, one elected starter under election-lock contention, election release after abrupt winner exit, connect-before-takeover behavior, detached child survival across starter exit, stale recovery, and bounded cleanup. Other rows prove only deterministic policy-fixture behavior here.
- The supervision fixture keeps the exact child unreaped until termination/wait, refuses a second disposition, records an already-exited child without signalling anything, and cannot affect a replacement even when a fixture reuses the numeric process identifier.
- Windows creation fixtures require `PIPE_REJECT_REMOTE_CLIENTS` on every path. A current-native explicit remote-client probe, when available, receives access denied while the local current-user client succeeds; an unavailable remote fixture produces no native rejection claim.
- Unsupported targets fail compilation through one explicit compile error and are absent from the documented supported matrix.
- Report fixtures accept zero or one current-environment observation and reject multiple, cross-target, wrong-source, copied, authority-labelled, or aggregate results.

- **Done when:** `cargo test -p slingshot-development --test platform_runtime_contract` passes deterministic policy fixtures for every abstract row and, only when one row matches the current environment, its real endpoint/locks/permissions/readiness/detachment/stable-supervision checks plus applicable Windows remote-client refusal, emitting at most one explicitly untrusted observation while every wrong-host, cross-compile, copied, PID-only cleanup, or aggregate claim fails.
