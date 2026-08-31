---
id: serve-legacy-initialized-revision
title: "Serve Legacy Initialized Revision"
workstream: "0029"
kind: task
depends_on:
  - frame-standard-stream-messages
gated: false
touches:
  - crates/slingshot-command-line/src/model_context_protocol/legacy_initialized_revision.rs
  - crates/slingshot-command-line/tests/legacy_initialized_revision.rs
  - "crates/slingshot-test-support/fixtures/model-context-protocol/official-schemas/2025-06-18/**"
status: done
merged_as: "5b124a640f73dc720f1d6ebdd78a25fa62c49a36"
---
# Serve Legacy Initialized Revision

Implement revision `2025-06-18` initialization, capability negotiation, initialized gate, ping, and shutdown behavior required by fsm.

**Steps:**

1. Commit the complete unmodified official revision `2025-06-18` schema artifact with source location, immutable retrieval identity, cryptographic digest, and exact bytes. Commit positive and malformed request/notification corpora for initialize with the exact supported version, initialize with a schema-valid unsupported version, initialized, ping, the exact tools/resources request inventory, cancellation/progress, schema-valid unsupported methods/capabilities, and every lifecycle/era error; record the exact applicable official definition for every case.
2. Implement the closed legacy lifecycle, exact `tools: {}`/`resources: {}` server capability set, and legacy result decorator while sharing framing, the ordered revision authority, and semantic payloads with the modern handler. Echo requested `2025-06-18`; for any other schema-valid requested version, successfully offer `2025-06-18` rather than returning an unsupported-version error, then await `notifications/initialized`.
3. Ensure the legacy decorator omits `resultType`, modern cache fields, and all other modern-only members while remaining valid against its official schemas.
4. Validate every positive inbound request/notification against its applicable committed official definition and prove every deliberately malformed inbound case fails that oracle; keep successful unsupported-version negotiation, schema-valid unavailable-method `-32601`, invalid-parameter `-32602`, and lifecycle-invalid cases distinct. Replay both supported-version and fallback-negotiated real-FSM-shaped `initialize`, `notifications/initialized`, direct `tools/call` sequences and assert no operation dispatch occurs before the initialized notification; cover `tools/list` in a separate synthetic caller sequence. Validate every emitted response against its applicable definition.

**Tests:**

- `legacy_initialized_revision` pins every accepted/rejected inbound request/notification classification, lifecycle transition, decorated response, and error byte against the complete official schema snapshot.
- Provenance tests recompute each committed schema digest; the real-FSM-shaped sequence reaches direct operation readiness only after the server's exact `2025-06-18` result and initialization, whether the client initially requested that value or another schema-valid version.
- Positive inbound cases satisfy their exact official definitions; malformed cases do not; schema-valid unsupported initialize version receives a successful `2025-06-18` result, while unavailable methods/capabilities and lifecycle-invalid cases reach only their separately pinned protocol errors.

- **Done when:** `cargo test -p slingshot-command-line --test legacy_initialized_revision` proves every accepted/rejected revision 2025-06-18 inbound request/notification classification, supported-version echo, schema-valid unsupported-version successful fallback, exact capability/method/error inventory, both real-FSM initialization/direct-call sequences, and every legacy response agree with the digest-pinned complete official schema with no modern-only member.
