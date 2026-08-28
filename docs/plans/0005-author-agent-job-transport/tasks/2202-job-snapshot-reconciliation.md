---
id: job-snapshot-reconciliation
title: "Job Snapshot Reconciliation"
workstream: "0022"
kind: task
depends_on:
  - agent-job-storage
gated: false
touches:
  - crates/slingshot-agent-connection/src/job_snapshot_reconciliation.rs
  - crates/slingshot-agent-connection/src/lib.rs
  - crates/slingshot-daemon/src/operation/job_reconciliation.rs
  - crates/slingshot-daemon/src/operation/mod.rs
  - crates/slingshot-agent-connection/tests/fixtures/job-snapshot-reconciliation/**
  - crates/slingshot-agent-connection/tests/job_snapshot_reconciliation.rs
status: planned
merged_as: ""
---
# Job Snapshot Reconciliation

Establish current remote truth after reconnection or daemon restart instead of assuming the event stream retained every transition.

**Steps:**

1. Commit initial-attachment immediate-terminal, same-state, advanced/snapshot-ahead state, physical retry Running, terminal-state, exact/missing/wrong CommandCanonicalJsonContractDigest, either schema-root annotation/role digest, SelectedCommandContractIdentity, or SubmittedCommandDigest on active/Retired lookup and snapshot including contract-only and limits-only mismatch, valid-shaped same-command/different-validated-arguments terminal substitution, stale-snapshot, wrong agent-operation/subscription/target, malformed, oversized, unauthorized-refresh, route-status/throttling/body-idle/deadline, active/Retired/missing operation, request-start-relative retention with delayed/equality body receipt, high-water reset, generation-change known-job recovered, generation-change known-job missing, generation-change ambiguous, canonical operation/job query, malformed/lowercase/overencoded/plus/duplicate/missing/surplus/double-encoded query, and redirect fixtures before implementation.
2. Adopt the dependency-ordered daemon operation root, declare `job_reconciliation` exactly once, and implement authenticated manifest-bounded logical lookup by AgentOperationIdentifier and, for generation-loss recovery, one fixed exact query per persisted physical Sling Job identifier, in canonical sorted order and within the physical-match/elapsed bounds, with redirects disabled.
3. Validate protocol/transport digest, separate canonical-contract digest, both schema-root annotations/role digests, target revision, subscription, logical identifier, generation, exact physical identifier, snapshot sequence, monotonic retry facts, request-start-derived retention, unchanged-five-field SelectedCommandContractIdentity, and SubmittedCommandDigest for every Found/Retired lookup and physical snapshot. Compare all with stored/recomputed artifact-authenticated raw-canonical command provenance before a nonregressive pure reconciliation.
4. After the digest match, hand bounded terminal data and snapshot facts to an injected terminal-settlement boundary and do not directly persist a terminal snapshot/state/result or notify waiters in this task. Task 2301 owns the concrete bounded wire conversion plus Plan 0003 `validate_result_for_command` implementation and the one resulting atomic snapshot/state/result-or-failure transaction; this task's fake boundary proves rejection leaves every snapshot/job/result fact unchanged. Ordinary snapshot retrieval never advances Last-Event-ID.
5. Implement cursor-expiry and integrity-conflict reset by calling the fixed authenticated high-water route with exactly the persisted DaemonSubscriptionIdentifier/AgentEventStoreGeneration query pair, validating its echoed pair and captured cursor, snapshotting every known nonterminal operation, requiring each snapshot watermark to cover at least that captured high-water, routing generation change to persisted Sling Job lookup, atomically installing generation/cursor/watermarks/job facts plus incident disposition only for complete reconciled truth, then reconnecting on the fixed event route with Last-Event-ID equal to that high-water and replaying only later events; one job snapshot or an older-watermark snapshot never heals a subscription cursor conflict.
6. Treat later events whose sequences are already covered by snapshot as stale cursor-only no-ops; require digest agreement whenever a retained ledger row exists.
7. Put same-generation missing logical lookup into durable grace/reconciliation: exact resubmission resolves never-submitted versus active versus Retired without a second effect. On generation change, recover every known physical identifier first; missing records and evidence-free ambiguity retain explicit indeterminate certainty before RemoteStateLost.

**Tests:**

- Same, advanced, and snapshot-ahead snapshots converge to exact durable state, sequence, generation, and watermark.
- A job that becomes terminal before initial stream attachment converges through its first snapshot without requiring a replayed event.
- Stale snapshots do not roll state or sequence backward.
- Terminal snapshots publish data only after SubmittedCommandDigest comparison and the injected terminal-settlement transaction succeeds; its rejection fixture proves no direct persistence here, while Task 2301 supplies the real bounded conversion and Plan 0003 request/result validator.
- Ordinary snapshot recovery preserves the stored cursor; reset installs only the authenticated, identity-bound captured high-water after every snapshot watermark covers it, and exact-route replay starts above it.
- Snapshot-ahead lower/equal unseen events are cursor-only no-ops, while retained rows reject a changed digest.
- A generation change during reset commits no unsupported resubmission; known Sling Jobs reconcile through their route, while known-missing and evidence-free ambiguous cases preserve distinct certainty/loss outcomes.
- Wrong-operation/subscription/target/transport/canonical-contract/annotation/role/digest, a schema-valid result substituted from the same command with different arguments, malformed/noncanonical query, oversized, redirect, grace-period missing, Retired, explicit loss, and invalid or delayed-to-zero request-start retention have distinct stable outcomes; query values decode exactly once and decoded Sling Job separators never alter route selection.
- Cloud unauthorized refresh follows the one-refresh rule while Basic unauthorized does not repeat.

- **Done when:** cargo test -p slingshot-agent-connection --test job_snapshot_reconciliation passes all transport/canonical-contract/dual-annotation/five-field/digest-gated snapshot-ahead and injected-terminal-settlement boundaries, physical-retry, high-water reset, generation-loss known-job recovered/missing and ambiguous cases, canonical single-decode lookup queries, same-generation Retired/grace, request-start-relative-retention, monotonicity, persistence-order, authentication, target-partition, bound, origin, and certainty/error-class cases without duplicating Task 2301's result validator.
