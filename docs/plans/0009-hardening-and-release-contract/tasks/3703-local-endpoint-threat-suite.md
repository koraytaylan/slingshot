---
id: local-endpoint-threat-suite
title: "Local Endpoint Threat Suite"
workstream: "0037"
kind: task
depends_on:
  - author-network-chaos
  - credential-filesystem-threat-suite
  - credential-exposure-threat-suite
  - daemon-process-chaos
  - storage-fault-chaos
gated: false
touches:
  - crates/slingshot-daemon/src/ownership.rs
  - crates/slingshot-daemon/src/local_server.rs
  - crates/slingshot-test-support/src/lib.rs
  - crates/slingshot-test-support/src/local_endpoint_attacker.rs
  - crates/slingshot-development/tests/local_endpoint_threats.rs
status: done
merged_as: ""
---
# Local Endpoint Threat Suite

The per-target endpoint is an operating-system-user control boundary. Processes running as the same account are inside the trust boundary and can already read that account's configuration and invoke retained control operations; Slingshot prevents accidental or cooperative ownership races and rejects other identities, malformed peers, and stale or cross-namespace protocol facts without claiming containment from a malicious same-account process.

**Steps:**

1. Commit attack cases first for another namespace, forged readiness and nonce, cooperative endpoint replacement, same-account Unix unlink, stale process evidence, replay, target/operation-version substitution, malformed hello, oversized cohorts, pre-hello operation frames, different operating-system identity, and Windows local-versus-remote named-pipe clients with/without `PIPE_REJECT_REMOTE_CLIENTS`. Add compiled CLI and both Model Context Protocol cleanup cases for responsive current-nonce stop, unresponsive retained child/native-instance handle termination-and-wait, stale nonce, owner replacement, and process-identifier reuse beside an unowned sentinel.
2. Build an attacker client that speaks raw local frames and can replace temporary runtime artifacts only inside its owned test root.
3. Exercise current-user endpoint permissions and peer identity. On the owner-mapped Windows native row, require the production pipe constructor's `PIPE_REJECT_REMOTE_CLIENTS`, prove local current-user success, and prove a separately provisioned remote client is rejected before framing; policy fixtures alone cannot satisfy that release case. Emit exact real-versus-policy row reports and retain the same-account trust limitation.
4. Add instrumentation proving rejected connections do not open configuration, SQLite, credential, executor, or author adapters, while a retained control client can obtain bounded status and stop an operation-incompatible daemon without reaching those adapters. Cleanup may address a responsive owner only through the exact current nonce and an unresponsive spawned owner only through the retained instance-bound supervision handle; process identifiers are recorded solely as diagnostics and are never enumerated, checked for liveness, or later signalled.
5. Harden ownership or server checks only where a retained attack case demonstrates a bypass.

**Tests:**

- Namespace and nonce mismatches, forged records, stale process identifiers, and pre-hello messages never dispatch.
- A stale nonce cannot stop a replacement; a retained handle cannot target another instance; process-identifier reuse cannot harm the unowned sentinel. Cleanup fails if either compiled client enumerates descendants by identifier, performs check-then-signal by identifier, loses the retained instance handle, or omits terminate-and-wait ownership accounting.
- Target-revision and operation-version mismatches never dispatch; retained hello, ping, status, and stop remain available for every retained control fixture.
- Slingshot's cooperative contender never replaces or deletes a live endpoint without the owner lock. A test-root external Unix unlink is diagnosed as same-account availability loss and cannot transfer the owner lock, forge readiness acceptance, or mutate durable operation state; no claim says the pathname is undeletable by its owning account.
- Excess connections receive the bounded refusal while existing and later valid clients remain serviceable.
- Peer-identity policy rejects a different operating-system identity on platforms that expose it and fails explicitly through the policy fixture elsewhere; matching identity is authorization at this boundary, not protection from another process under that identity.
- Windows release evidence fails if any constructor omits `PIPE_REJECT_REMOTE_CLIENTS`, if only an access-control-list fixture is supplied, or if the real owner-mapped remote client connects; the authenticated row must show local success and remote pre-frame refusal.
- Every rejected case records zero downstream adapter access.
- Every supported native row emits the exact real-versus-policy case inventory; task `release-artifact-contract` reruns the real local-endpoint subset and binds that report into authenticated row evidence.

- **Done when:** `cargo test -p slingshot-development --test local_endpoint_threats` passes every in-scope attack and compiled CLI/Model Context Protocol cleanup race with current-nonce or retained-instance-handle authority only, diagnostic-only process identifiers, stale-nonce/replacement/PID-reuse safety, zero unauthorized downstream access, cooperative live-owner preservation, bounded cohorts, and platform-appropriate operating-system-user enforcement, and `scripts/quality` succeeds.
