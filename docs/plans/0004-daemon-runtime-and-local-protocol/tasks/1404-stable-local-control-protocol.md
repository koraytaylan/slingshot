---
id: stable-local-control-protocol
title: "Stable Local Control Protocol"
workstream: "0014"
kind: task
depends_on:
  - bounded-local-frame-codec
gated: false
touches:
  - crates/slingshot-local-protocol/src/control.rs
  - crates/slingshot-local-protocol/tests/control.rs
  - "crates/slingshot-local-protocol/tests/fixtures/control/**"
status: planned
merged_as: ""
---
# Stable Local Control Protocol

A client must be able to inspect and explicitly stop a live daemon even when the client and daemon share no compatible operation-protocol version. This task extends Plan 0001's retained control surface independently of operation messages without replacing its manifest version, wire spellings, limits, or instance authorization.

**Steps:**

1. Author canonical principal-bound hello, `daemon.ping`, exact-current-nonce `daemon.stop`, stale-prior-instance stop, daemon-status, malformed, raw-principal-field, operation-version and daemon-runtime-digest incompatibility, and hello-required-before-operation fixtures before defining the control types.
2. Reuse Plan 0001's `daemon.ping` and `daemon.stop` serialized spellings and the embedded typed `FoundationContract` control version, request/method/error bounds, nonce representation, and collection bound. Extend the closed stable control envelope with full-word Rust identifiers and caller-created request identifiers; do not introduce a Plan-0004 control version or copied default.
3. Make hello expose the runtime namespace, instance nonce, product version, selected `AuthorTargetIdentity`, `SelectedEnvironmentRevision`, exact `DaemonRuntimeContractDigest`, and the closed supported operation-version set.
4. Require hello to be the first complete request on a connection, then keep `daemon.ping`, bounded daemon status, and `daemon.stop` decodable and dispatchable when the operation-version sets do not intersect. Stop authorizes only the exact nonce returned by the live hello/readiness instance; contextual namespace/target values and diagnostic process identifiers cannot substitute for it.
5. Return the retained `stale_daemon_instance` error before side effects for every wrong or prior-instance nonce, including after replacement with a reused diagnostic process identifier. Reject unknown fields, unknown methods, target values in stop that differ from hello, oversized advertised sets, and invalid identifiers with stable manifest-bounded errors.

**Tests:**

- Every retained control request and response round-trips to its canonical fixture bytes.
- A client with no compatible operation version or daemon-runtime digest can complete hello, `daemon.ping`, daemon status, and exact-current-nonce `daemon.stop`.
- Hello's complete opaque target identity and selected-environment revision are mandatory and byte-stable; no downstream principal member or raw Basic/Cloud field is accepted.
- A wrong or stale nonce returns `stale_daemon_instance` without stopping admission, closing a listener, removing readiness, signalling a process, or affecting a replacement; malformed control version, unknown method, and over-bound version sets fail without being interpreted as operation messages.
- A non-hello first request is refused before dispatch, giving the server one unambiguous pre-hello deadline boundary.

- **Done when:** `cargo test -p slingshot-local-protocol --test control` byte-matches every retained-control fixture from the embedded typed foundation values and proves inspection plus nonce-bound `daemon.stop` remain usable under complete operation-protocol incompatibility while a stale nonce cannot affect a replacement, and all workspace gates succeed.
