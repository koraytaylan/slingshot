---
id: agent-store-and-logical-execution-contract
title: "Agent Store and Logical Execution Contract"
workstream: "0019"
kind: task
depends_on:
  - continuation-key-lifecycle-contract
gated: false
touches:
  - crates/slingshot-development/tests/fixtures/workspace-module-map/module-ownership.txt
  - crates/slingshot-domain/src/lib.rs
  - crates/slingshot-domain/src/logical_agent_operation.rs
  - crates/slingshot-domain/src/remote_job.rs
  - crates/slingshot-domain/tests/remote_job.rs
  - crates/slingshot-agent-protocol/src/job_contract.rs
  - crates/slingshot-agent-protocol/src/lib.rs
  - crates/slingshot-agent-protocol/tests/agent_store_and_logical_execution_contract.rs
  - crates/slingshot-agent-protocol/tests/fixtures/agent-store-and-logical-execution/**
  - schemas/agent-protocol/job/**
status: done
merged_as: "ab3c641805a880f6317f659c6d4a94abe2784da1"
---
# Agent Store and Logical Execution Contract

Sling delivery is physically at least once, so the protocol must bind duplicate physical records to one durable logical operation and one possible command effect.

**Steps:**

1. Adopt the dependency-ordered domain crate root, declare `logical_agent_operation` exactly once while retaining Plan 0001's existing `remote_job` declaration, and define the architecture's exact logical-operation/outbox/physical-attempt states, six Sling Job properties, derivation, revisions, sorted identifiers, and closed mismatch/timeout/over-count/attempt-exhaustion outcomes.
2. Commit reservation and outbox before JobManager; reconcile a bounded query with exact postchecks; allow multiple physical records/attempts while retaining one logical operation and never claiming physical exactly-once delivery.
3. Define cluster-wide worker lease/CAS/fence and the durable no-return checkpoint. After `ExecutionStarted`, no expiry/retry may re-execute; loss becomes reconciliation or fail-closed Indeterminate. Require every multi-step external executor to gate each effect with the same fence/checkpoint or refuse compatible readiness.
4. Define generation/current-and-prior capacity, deterministic worst-case reservations, retention, compaction, rotation, exhaustion, and exact logical store states from the typed transport contract and installed capacity document.
5. Generate job/store schemas and vectors. Rust tests prove language-neutral transitions and FakeAuthor compatibility, not Java Sling/JCR atomicity.

**Tests:**

- Crash before/during/after JobManager call yields zero, one, or multiple physical records but one logical reservation; bounded exact matches associate, mismatch/ambiguity fails closed, and no path creates a second effect.
- Two consumers, lease loss, stale fence, node crash/replacement, retry/requeue, and terminal-CAS races produce at most one `ExecutionStarted` effect attempt; post-start loss never authorizes takeover.
- Exact capacity/retention/generation maxima pass and the next unit refuses before partial state; Duplicate/Retired replay remains available at full capacity.

- **Done when:** focused domain/protocol tests prove logical-operation/outbox crash recovery, physical-at-least-once honesty, fenced single-effect safety, complete capacity/generation semantics, and schema parity, and all workspace gates succeed.
