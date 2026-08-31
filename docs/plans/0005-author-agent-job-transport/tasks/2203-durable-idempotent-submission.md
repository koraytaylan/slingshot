---
id: durable-idempotent-submission
title: "Durable Idempotent Submission"
workstream: "0022"
kind: task
depends_on:
  - authenticated-command-submission
  - job-snapshot-reconciliation
gated: false
touches:
  - crates/slingshot-storage/migrations/**
  - crates/slingshot-storage/src/database.rs
  - crates/slingshot-storage/src/sqlite_statement_inventory.rs
  - crates/slingshot-storage/src/operation/remote_submission.rs
  - crates/slingshot-storage/tests/migrations.rs
  - crates/slingshot-storage/src/operation/mod.rs
  - crates/slingshot-daemon/Cargo.toml
  - crates/slingshot-daemon/src/operation/remote_submission.rs
  - crates/slingshot-daemon/src/operation/mod.rs
  - crates/slingshot-daemon/tests/fixtures/durable-idempotent-submission.jsonl
  - crates/slingshot-daemon/tests/durable_idempotent_submission.rs
status: planned
merged_as: ""
---
# Durable Idempotent Submission

Close daemon/HTTP/Sling handoffs while honestly allowing duplicate physical Sling Job records and preserving one cluster-wide logical operation and at most one command effect.

**Steps:**

1. Commit fixtures for Plan 0004 admission/capability/remote-child/POST boundaries; exact canonical artifact, each schema-root annotation/role digest, and raw command canonicality; transport/canonical-contract/unchanged-five-field/schema/SubmittedCommandDigest drift including contract-only drift; equal and independently changed typed AuthorTargetIdentity/SelectedEnvironmentRevision values; same-generation restart; active/Retired reservations; capacity refusal; request-start retention; generation rotation/loss/exhaustion; and every logical-operation/outbox/JobManager/worker-lease crash boundary.
2. Adopt the dependency-ordered storage and daemon operation roots and declare each crate's `remote_submission` leaf exactly once. Require the admitted InstallationIdentifier, opaque typed AuthorTargetIdentity, SelectedEnvironmentRevision, validated Command, exact Daemon/AuthorAgent contract digests, exact CommandCanonicalJsonContractDigest, and both annotated role schemas before capability network access. After capability and before POST, atomically persist the generation-scoped remote child, derivation preimages, exact canonical command bytes, transport/canonical-contract/unchanged-five-field identities, SubmittedCommandDigest, ExpectedArtifactManifest, retry facts, and request-start-derived relative retention. No remote response supplies a derivation preimage.
3. Before every retry, recompute and compare every persisted identity/digest/revision, authenticate the exact canonical artifact and both schema annotations/role digests, validate stored raw bytes, apply Draft 2020-12 decoded shape, then typed conversion. Drift refuses byte-preservingly before provider/network; an equal opaque target/revision under genuine same-principal refresh changes none of those values. This task never reconstructs Plan 0002's preimage fields.
4. Treat POST ambiguity through same-tuple lookup, never replacement identity. Reconcile the external agent's logical-operation/outbox contract: exact bounded physical matches may contain multiple Sling Job identifiers; persist their sorted set. Mismatch, timeout, or over-count remains fail-closed. Zero matches can request a checked next physical attempt only while remote state proves `ExecutionNotStarted`.
5. Persist worker fence and `ExecutionStarted` no-return checkpoint. No lease expiry, physical requeue, retry, restart, or node replacement after that checkpoint authorizes another command effect; unresolved post-start truth is Indeterminate/RemoteOutcomeUnknown. Exact Duplicate/Retired replay remains available even at full capacity.
6. On generation change prohibit resubmission or replacement derivation for existing work. Use all known bounded physical identifiers for snapshot recovery; absent truth becomes the exact fail-closed disposition. A fresh local operation may use the new generation only through ordinary admission.

**Tests:**

- No POST occurs without a complete local remote child and exact transport/command provenance; every restart recomputes byte-identically.
- JobManager crash-before/during/after fixtures produce zero, one, or multiple physical records but one logical reservation; two consumers/stale fences/requeues yield at most one `ExecutionStarted` effect attempt.
- Contract-only or annotation-only drift and noncanonical-but-schema-equivalent command bytes fail before decoded schema/typed conversion and before network/reservation mutation.
- Equal supplied target/revision remains compatible; an independently changed supplied revision refuses old nonterminal work before provider/network, and a changed opaque target is disjoint. No upstream identity/revision fields are duplicated here.
- Retired/capacity/generation loss never creates replacement work, while a known physical record can recover authoritative truth.

- **Done when:** `cargo test -p slingshot-daemon --test durable_idempotent_submission` proves exact artifact/dual-annotation/unchanged-five-field provenance, raw-canonical-before-Draft-before-typed validation, request-start retention, logical/outbox crash recovery, possible duplicate physical records with one fenced effect attempt, opaque target/revision drift behavior, generation/capacity safety, and no unsafe replacement submission, and all workspace gates succeed.
