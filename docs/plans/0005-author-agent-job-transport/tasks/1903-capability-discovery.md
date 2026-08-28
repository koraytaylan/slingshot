---
id: capability-discovery
title: "Capability Discovery"
workstream: "0019"
kind: task
depends_on:
  - agent-store-and-logical-execution-contract
  - author-hypertext-transfer-protocol-policy
  - author-request-authentication
  - author-cross-site-request-forgery-protection
  - fake-author
gated: false
touches:
  - crates/slingshot-agent-connection/src/capability_discovery.rs
  - crates/slingshot-agent-connection/src/lib.rs
  - crates/slingshot-agent-connection/tests/fixtures/capability-discovery/**
  - crates/slingshot-agent-connection/tests/capability_discovery.rs
status: planned
merged_as: ""
---
# Capability Discovery

Reject an incompatible author agent before submitting a command whose name or schema it does not support.

**Steps:**

1. Commit compatible, missing/malformed/changed `AuthorAgentTransportContractDigest`, separate `CommandCanonicalJsonContractDigest`, each schema-root annotation/role digest, current/changed/invalid/exhausted generation, missing-command or changed unchanged-five-field identity, continuation deployment-profile/authority/readiness mismatch, missing logical-operation/outbox/worker-fence or lookup guarantee, absent/invalid current/global-prior capacity and retention, bare-ingress/status/deadline/version/body/media/redirect fixtures before implementation. Every Plan-0005 boundary comes from the typed transport contract and command provenance comes from Plan 0003's exact artifacts.
2. Implement bounded capability retrieval from the exact author route with redirects disabled. The cross-site-request-forgery dependency serializes their shared `slingshot-agent-connection/src/lib.rs` export only; capability GET performs no token preflight.
3. Require exact agent version, `AuthorAgentTransportContractDigest`, separate `CommandCanonicalJsonContractDigest`, and both annotated role schemas/digests before current AgentEventStoreGeneration, logical-operation/Retired reservation, bounded physical match/outbox attempt and worker-fence guarantees, exact installed current/prior capacities within manifest hard caps, progress coalescing, no-retention-loss rotation, both lookup routes, and positive retention. Require one compatible continuation deployment profile backed by the same authenticated cluster-capable durable linearizable authority contract with matching identity/revision/fence/format, including AEM 6.5 single-node; absent, unready, unauthenticated, nondurable, or nonlinearizable authority state cannot advertise compatibility. Construct the unchanged SelectedCommandContractIdentity only after all five Plan 0003 fields match. No token, limit, or canonical-contract alias is negotiated.
4. Cache a validated capability document only for ordinary work in one target connection; every recovery submission refreshes current generation, and an explicit incompatibility or changed generation invalidates the cache before POST.
5. Return stable compatibility errors that name commands and digest mismatch without including credentials or response bodies.

**Tests:**

- A fully matching fake author validates every Plan 0003 command.
- Missing command, mismatch in transport/canonical-contract/annotation/role/five-field compatibility, absent universal authority/reservation/Retired/capacity/rotation or lookup guarantee, invalid checked capacity/generation/request-start-relative retention, changed generation for ambiguous recovery, and unsupported protocol version fail before any recovery submission.
- Bare 404 reports AuthorIngressRouteUnavailable; redirect, wrong content type, malformed JSON, and over-bound response fail closed without alternate-route probing.
- Capability order does not affect comparison, while duplicate capability names are rejected.
- The fake author records zero publisher connections and no credential values appear in errors.

- **Done when:** `cargo test -p slingshot-agent-connection --test capability_discovery` proves exact transport/separate-canonical-contract/dual-annotation/unchanged-five-field compatibility, universal continuation-authority readiness, logical/outbox/fence and generation gates, manifest-bounded capacities/request-start retention/rotation, dual lookup, replay/reset, redirect, ordering, redaction, and author-only cases.
