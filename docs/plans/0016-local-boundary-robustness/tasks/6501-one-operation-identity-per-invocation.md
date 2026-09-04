---
id: one-operation-identity-per-invocation
title: "One Operation Identity Per Invocation"
workstream: "0065"
kind: task
depends_on: []
gated: false
touches:
  - crates/slingshot-command-line/src/application.rs
  - crates/slingshot-command-line/src/command_line.rs
  - crates/slingshot-command-line/tests/application_dispatch.rs
  - crates/slingshot-command-line/tests/exits_and_interrupts.rs
status: completed
merged_as: ""
---
# One Operation Identity Per Invocation

Request identifiers are millisecond timestamps and are regenerated at multiple phases. Two invocations can collide, while retry advice from one invocation can name a freshly generated value rather than the operation key already sent.

**Steps:**

1. Inject a collision-resistant identifier generator and create one request identity at the invocation boundary; do not derive uniqueness from wall-clock granularity.
2. Thread that exact value through daemon start/hello, request envelope, generated operation key, observation, cancellation, interrupt phase, and retry rendering without another generator call.
3. Preserve caller-supplied idempotency keys and distinguish them explicitly from generated request correlation where the protocol requires both.
4. Freeze time, run concurrent invocations and processes, and interrupt before send, after send, and before receipt; require unique new operations and byte-identical retry identifiers for the operation actually submitted.

- **Done when:** every invocation has one stable noncolliding identity across all phases, and every retry instruction names exactly the durable operation key that may already have taken effect.
