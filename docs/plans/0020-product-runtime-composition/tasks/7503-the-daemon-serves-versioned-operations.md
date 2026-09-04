---
id: the-daemon-serves-versioned-operations
title: "The Daemon Serves Versioned Operations"
workstream: "0075"
kind: task
depends_on: ["readiness-follows-complete-durable-startup"]
gated: false
touches:
  - crates/slingshot-daemon/src/service.rs
  - crates/slingshot-daemon/src/request_dispatch.rs
  - crates/slingshot-daemon/tests/request_dispatch.rs
status: planned
merged_as: ""
---
# The Daemon Serves Versioned Operations

The product service dispatches only ping and stop and advertises no operation version, while every CLI operation first asks hello and then sends a versioned `OperationEnvelope`.

**Steps:**

1. Add hello and versioned `OperationEnvelope` dispatch to the product service, using exact command registry, limits, schema identity, and Plan 0019 repository methods.
2. Persist accepted submission before acknowledgement and expose bounded status, wait/progress, result, artifact, cancellation, recovery, and maintenance methods through the retained local protocol.
3. Advertise exactly the operation versions installed in this runtime and return typed unavailable, unsupported-version, validation, and domain refusals rather than method-not-found or an empty hello.
4. Drive every method across a real framed endpoint, including malformed/foreign envelope, exact/adjacent version, unavailable executor, backpressure, cancellation, and shutdown cases.

- **Done when:** hello truthfully advertises the installed operation protocol and every public operation/control method crosses the real framed service to one persisted result or typed refusal, with no ping/stop-only fallback.
