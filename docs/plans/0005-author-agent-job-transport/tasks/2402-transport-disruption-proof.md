---
id: transport-disruption-proof
title: "Transport Disruption Proof"
workstream: "0024"
kind: task
depends_on:
  - author-agent-conformance
gated: false
touches:
  - crates/slingshot-daemon/Cargo.toml
  - crates/slingshot-daemon/tests/fixtures/transport-disruption-proof.jsonl
  - crates/slingshot-daemon/tests/transport_disruption_proof.rs
status: done
merged_as: "8374c48edf050abc674fe4ed123a26249ba696ab"
---
# Transport Disruption Proof

Exercise every durable/network/topology handoff under termination, replacement, or disconnection and prove terminal truth with physical at-least-once delivery but no second logical command effect.

**Steps:**

1. Commit a deterministic matrix for exact AuthorAgentTransportContract boundaries/digest, separate command-canonical artifact/dual annotations and contract-only drift, raw-canonical-before-Draft-before-typed command/result gates, opaque target/SelectedEnvironmentRevision/transport/unchanged-five-field/SubmittedCommandDigest persistence, POST/event/snapshot/artifact crash order, selected-snapshot TLS with distinct provider-policy-verified platform Identity-Management-Services roots and effective platform-plus-selected-additional-author-CA author roots, no reload/merge/cross-use, hostile additional-author-CA Identity-Management-Services interception, direct-or-explicitly-warned-cleartext author auth/protection/status, HTTP/1.1-or-HTTP/2-only negotiation with no upgrade/migration, encoded/decoded head bounds, first-informational/trailer/ambiguous-framing/trailing-byte failure at each route boundary, and distinct DNS/TCP/TLS/request/header/body deadlines/routes, capacity/request-start-retention/generation rotation, event reset/compaction, exact load and inline/operation-free-associated maintenance metadata/read/preview/apply/replay/restart/retirement branches, terminal evidence, and every Plan 0003 result/failure/disposition.
2. Cover all continuation profiles through the same cluster-capable durable linearizable authority contract, including single-node AEM 6.5; no current-node-count or deployment observation may relax it. Cover concurrent initialization; CAS conflict/timeout/ambiguity; stale fence; authority absence/orphan/corruption/unsafe access; exact 431/768-byte states; early/equality/exhausted rotation; multi-node issuance/validation; rolling versions; restart, node replacement, and scale-out. Rust/FakeAuthor makes no Java/AEM/provider-execution claim.
3. Cover logical operation/outbox/JobManager/worker checkpoints before and after each durable write/call. Script zero, one, and multiple exact physical matches, mismatch, over-count, timeout, physical requeue, competing workers, lease loss, stale fence, post-`ExecutionStarted` crash, terminal CAS, and restart/replacement. Record logical/outbox revisions, sorted physical set, fences, and effect count.
4. Cover same-target canonical-equivalent metascope, `VerifiedIdentityManagementTrustPolicyIdentity`, and `VerifiedAuthorTrustPolicyIdentity`; genuine same-principal credential rotation; independently changed metascope, either named trust-policy identity, and principal. Drift refuses before network without rewriting old work; additional author authority never validates Identity Management Services.
5. Cover side-effect-free escaped OSGi PID lookup/filter injection, single `Configuration.getProperties()` dictionary, key-first/metatype-plan-before-visible-value acquisition, and every provider/designate/factory/bundle-location/metatype-first redaction edge; hostile FileVault regex/XML/profile/budget/no-widening cases; pre-mutation `parent_not_orderable`; exact 262,144/262,145 load branches; byte-identical inline/externalized maintenance, target-and-identifier metadata, unchanged and sole ownership-transfer lookup/start races, supersession/retirement before start, and no hash inversion; inline/externalized/remote artifact parity; and all closed failures.
6. Run every row twice in a fresh target root with injected clocks/randomness and explicit readiness, never correctness-sensitive sleeps.

**Tests:**

- Retained truth converges to its authoritative terminal outcome; unavailable truth remains exact RecoveryRequired or fail-closed disposition without invented certainty.
- Duplicate physical records/attempts may exist, but one target/revision/transport/generation AgentOperationIdentifier has one logical record and at most one fenced `ExecutionStarted` effect attempt; post-start loss never authorizes takeover.
- Capacity/refusal/rotation is atomic; Duplicate/Retired replay remains available at full capacity; no generation/key/fence counter wraps or reuses.
- Current/previous tokens validate on every ready instance and after restart/replacement; every profile uses the universal authority, while profile/provider/durability/linearizability/authorization/readiness failures issue no token and admit no job.
- Every malformed post-byte branch with and without physical creation reconciles by the same logical tuple and never silently terminalizes or creates replacement work.
- Every strict-protocol/informational/trailer/framing/compressed-head/trailing-byte cut exposes no finite partial result or artifact; after a POST byte it preserves SubmissionUnknown, while non-POST retry/certainty remains route-specific and event-stream trailers create no synthetic cursor fact.
- OSGi/FileVault/add-component and every other registered failure retains exact fields, disposition, no-partial semantics, and envelope/externalization parity; secret scans remain clean.
- No row dials publisher/proxy, aliases a route, repairs noncanonical command bytes, or records credentials/key material.
- Repeating the seed yields byte-identical state/request traces.

- **Done when:** `cargo test -p slingshot-daemon --test transport_disruption_proof` passes every manifest/artifact/annotation provenance, opaque revision, universal deployment-profile authority, logical outbox/physical attempt/worker fence, live/restart/replacement, capacity/rotation/request-start retention, selected-snapshot/strict-framing/split-phase response/routing/event, dictionary/Plan 0003 failure, load/maintenance-metadata-read/result/artifact checkpoint twice with possible duplicate physical records but one effect and author-only secret-free traffic, and all workspace gates succeed.
