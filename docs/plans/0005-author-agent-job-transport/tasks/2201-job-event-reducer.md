---
id: job-event-reducer
title: "Job Event Reducer"
workstream: "0022"
kind: task
depends_on:
  - event-stream-reconnection
gated: false
touches:
  - crates/slingshot-agent-connection/src/job_event_reducer.rs
  - crates/slingshot-agent-connection/src/subscription_event_fold.rs
  - crates/slingshot-agent-connection/src/lib.rs
  - crates/slingshot-agent-connection/tests/fixtures/job-event-reducer.jsonl
  - crates/slingshot-agent-connection/tests/job_event_reducer.rs
status: done
merged_as: "5d84a241b2fbea667831ff3b7510c6b8e4a971be"
---
# Job Event Reducer

Reduce duplicate, delayed, skipped, snapshot-covered, unknown-operation, conflicting, and terminal events through pure job and subscription folds.

**Steps:**

1. Commit every allowed state transition, physical Sling retry/requeue as Running, regressive attempt/progress, forbidden Running-to-Queued, duplicate/conflict/compaction/gap/unknown/terminal cases, and exact/missing/wrong AuthorAgentTransportContractDigest, separate CommandCanonicalJsonContractDigest, either schema-root annotation/role digest, SelectedEnvironmentRevision, unchanged-five-field SelectedCommandContractIdentity, or SubmittedCommandDigest including contract-only, limits-only, and same-command/different-argument substitutions.
2. Implement a pure job reducer over domain RemoteJobObservation plus one validated event and a separate pure subscription fold over generation/cursor/digest/association facts.
3. Return Applied, ExactReplay, StaleCursorOnly, NeedsSnapshot, or IntegrityConflictNeedsReconciliation; equality against an existing ledger row succeeds only when canonical digest agrees.
4. Treat a forward per-job gap as NeedsSnapshot without applying job state. Treat a lower/equal sequence absent from the compacted ledger but covered by a snapshot watermark as StaleCursorOnly without requiring an unavailable digest.
5. Let a valid event for an unassociated AgentOperationIdentifier produce only cursor/canonical-digest disposition without terminal payload; later association uses lookup/snapshot. For associated work require exact transport digest, separate canonical-contract digest, both authenticated schema-root annotations/role digests, selected revision, unchanged-five-field selected identity, and command digest before either fold advances; mismatch records one bounded incident with both unchanged. Wrong subscription, target, revision, generation, or physical-attempt association is invalid.
6. Keep physical retry/backoff/requeue after first start logically Running with optional monotonic remote-attempt/progress metadata; reject Running to Queued and progress/attempt regression.
7. Make terminal states immutable, leave an equal-cursor conflict unchanged with a subscription-integrity-reset directive rather than an affected-job recovery directive, and keep connection health absent from RemoteJobState.

**Tests:**

- Queued, Running, Succeeded, and Failed transition fixtures cover the full domain-owned allowed table.
- Exact retained equal-sequence events are replays; a conflicting retained job sequence requests job reconciliation, while an equal-cursor digest conflict requires full-subscription reset without advancing state or cursor.
- Lower/equal unseen sequences covered by a snapshot are stale cursor-only no-ops; an existing row still requires an exact digest.
- Sequence gaps request a snapshot and do not mutate stored state.
- Physical requeue preserves logical Running and monotonic attempt/progress; Running-to-Queued is rejected without preventing a later authoritative snapshot reconciliation.
- Unknown-operation events, including terminal-shaped events whose terminal payload is not retained locally, plus already-terminal and stale events can advance the independent subscription fold; later association uses lookup/snapshot. An associated wrong-digest terminal event advances neither fold, and wrong-subscription events cannot advance either fold.
- No event transitions away from Succeeded or Failed, and no conflict directly invents a terminal disposition.

- **Done when:** cargo test -p slingshot-agent-connection --test job_event_reducer passes the exhaustive domain transition including physical retry mapping, digest-gated terminal data, independent subscription fold, snapshot-ahead, unknown-event association, substitution/conflict degradation, sequence-gap, monotonic attempt/progress, and terminal-immutability cases.
