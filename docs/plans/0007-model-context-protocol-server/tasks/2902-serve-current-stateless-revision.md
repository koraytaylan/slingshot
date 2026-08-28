---
id: serve-current-stateless-revision
title: "Serve Current Stateless Revision"
workstream: "0029"
kind: task
depends_on:
  - frame-standard-stream-messages
gated: false
touches:
  - crates/slingshot-command-line/src/model_context_protocol/current_stateless_revision.rs
  - crates/slingshot-command-line/tests/current_stateless_revision.rs
  - "crates/slingshot-test-support/fixtures/model-context-protocol/official-schemas/2026-07-28/**"
status: planned
merged_as: ""
---
# Serve Current Stateless Revision

Implement revision `2026-07-28` discovery and per-request metadata validation without connection initialization state or server-initiated requests.

**Steps:**

1. Commit the complete unmodified official revision `2026-07-28` schema artifact with source location, immutable retrieval identity, cryptographic digest, and exact bytes. Commit positive and malformed request/notification corpora for the exact request inventory `server/discover|ping|tools/list|tools/call|resources/list|resources/templates/list|resources/read`, inbound `notifications/cancelled`, outbound `notifications/progress`, required per-request metadata, direct requests, and era conflict. Add schema-valid unsupported-version and unsupported-method/capability cases, and record the exact applicable official definition for every case.
2. Implement modern era selection, per-request revision/capability validation, stateless dispatch eligibility, and one modern result decorator over semantic payloads. Discovery advertises exactly `supportedVersions: ["2026-07-28","2025-06-18"]` in shared-authority order plus only `tools` and `resources` server capabilities. An unsupported requested revision emits exact `UnsupportedProtocolVersionError` `-32022` with the same ordered `supported` array and exact requested string. A schema-valid unavailable server method/capability emits `MethodNotFoundError` `-32601`.
3. Require `resultType: "complete"` on every successful JSON-RPC result, including `tools/call` results whose tool-level `isError` is true; add revision-required `ttlMs`, `cacheScope`, and response metadata to list/read results as applicable.
4. Before dispatching each positive fixture, validate its exact request/notification bytes against the applicable definition in the committed official schema. Prove each deliberately malformed fixture fails that oracle, but keep schema-valid unsupported version, method/capability, and lifecycle/era conflicts as distinct dispatch errors. Pin the complete reachable standard-stream error inventory: `ParseError` `-32700`, `InvalidRequestError` `-32600`, `MethodNotFoundError` `-32601`, `InvalidParamsError` `-32602`, `InternalError` `-32603`, and `UnsupportedProtocolVersionError` `-32022`; prove header mismatch and missing-required-client-capability branches are unreachable. Validate every emitted success/error against its applicable official definition and assert request order and prior requests cannot alter a later request's validation result.

**Tests:**

- `current_stateless_revision` pins the exact ordered discovery versions, exact tools/resources capability object, complete supported request/notification inventory, every accepted/rejected inbound classification, metadata, every required result decoration, and every reachable error byte against the complete official schema snapshot.
- Provenance tests recompute each committed schema digest before using it as an oracle.
- Positive inbound cases satisfy their exact official definitions; malformed cases do not; an unsupported revision yields only exact `-32022` with `["2026-07-28","2025-06-18"]`, and a schema-valid unadvertised method/capability yields only exact `-32601`. Permutation cases prove validation is per request and the server emits no request messages.

- **Done when:** `cargo test -p slingshot-command-line --test current_stateless_revision` proves every accepted/rejected revision 2026-07-28 inbound request/notification classification, exact discovery/capability/method/error inventory, ordered two-revision response/error list, and every outbound response agrees with its digest-pinned complete official schema, all successful results carry required modern decoration, and request eligibility depends only on current metadata.
