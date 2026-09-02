---
id: operation-and-job-state-properties
title: "Operation And Job State Properties"
workstream: "0035"
kind: task
depends_on:
  - model-context-protocol-fuzzing
gated: false
touches:
  - crates/slingshot-development/Cargo.toml
  - crates/slingshot-development/tests/operation_and_job_properties.rs
  - policy/workspace-capabilities.toml
  - crates/slingshot-development/tests/fixtures/workspace-capability-inventory/consumer-capabilities.toml
  - "crates/slingshot-development/tests/fixtures/state-properties/**"
status: done
merged_as: "6d3dd105b8204db366049b0d56d84a9dc4349fea"
---
# Operation And Job State Properties

Example transitions do not cover the combinations created by retries, snapshots, scheduler cuts, and restart. This task generates those sequences and checks the durable invariants shared across plans.

**Steps:**

1. Commit generator seeds and minimized counterexample fixtures first for lifecycle transitions, every operation certainty, both `RecoveryExecutionEvidence` variants, every terminal kind/disposition pair including `ResultUnavailable`/`AuthoritativeRemoteSuccess`, post-success result/artifact acquisition and `PersistentCapacityUnavailable`, recovery-required and retry facts, all three continuation deployment profiles through the identical cluster-capable durable linearizable authority and authority/rotation/restart/scale-out cuts, logical-operation/outbox/physical-attempt/worker-fence states, exact token-validation precedence, configuration no-partial state, package staging/publication dispositions, non-orderable-parent preflight, subscription cursors and stream generations, rolling event digests, snapshot watermarks, ignored events, submissions, callers, queue capacity, artifacts, maintenance, and daemon, agent, or worker-node restart cuts.
2. Generate legal and illegal operation/job/recovery sequences across the complete conditional evidence matrix with every semantic size and iteration count expressed as a named test constant.
3. Compare incremental folding with replay from the same durable prefix, snapshot-and-compaction recovery, and a fresh scheduler over the same observation.
4. Generate repeated operation identifiers with equal and unequal fingerprints and assert repository replay/conflict behavior.
5. Persist every minimized failure as a named regression fixture and print its seed.

**Tests:**

- Operation revisions, subscription stream cursors, snapshot watermarks, and independent per-job sequences never regress; terminal states never change.
- Duplicate equal facts are no-ops and conflicting equal-sequence facts fail without mutation.
- A snapshot-ahead lower or equal unseen event is a stale cursor-only fact, while a retained equal-sequence digest must still match; rolling compaction never wedges later events.
- Incremental, reopened, and replayed folds agree for every generated durable prefix.
- Scheduler directives never exceed bounds, preserve per-caller order, and service every continuously eligible caller.
- Equal identifier/fingerprint pairs create one operation; unequal pairs conflict.
- Zero, one, or the manifest-bounded maximum duplicate physical Sling Job records remain associated with one logical reservation, and no generated outbox/requeue/restart/node-replacement schedule crosses the fenced `ExecutionStarted` checkpoint twice or produces more than one command-effect attempt.
- Maintenance never removes nonterminal operations or artifacts reachable from retained operation results, and a retry always preserves the persisted agent operation identifier.
- For each of `aem_6_5_single_node_shared_secret_authority_v1`, `aem_6_5_cluster_shared_secret_authority_v1`, and `aem_cloud_service_shared_secret_authority_v1`, the initialized canonical continuation key ring reaches its independently reconstructed exact 431-byte maximum, reserves that capacity before readiness, advances `continuation_key_<nonzero-u64>` monotonically, and refuses unchanged at generation exhaustion. The same deployment-wide cluster-capable durable linearizable authority contract applies regardless of observed node count; private or node-local state is never compatible. Rotation retains the previous key for exactly 960,000 milliseconds across restart, refuses too early, retires at equality, and exposes only a completely durable old or new ring across every replacement cut. Missing/unreadable/noncanonical/over-bound/permission-unsafe initialized state refuses readiness and admission without regeneration. Malformed framing wins before integrity; all 32 tag bytes use a constant-time comparison; unknown-key/tag failure wins before authenticated payload checks; then wrong target, wrong query, and expiry occur in that order. No adapter decodes or rewrites the token, and key loss never silently creates authority over an outstanding token.
- Lookup/configuration budget failure commits no partial observation; package pre-publication and publication-unknown states never collapse; and `parent_not_orderable` is generated only before InFlight with authoritative proof of no content or ordering effect.
- Ambiguous submission and known-live-job transport loss retain `ExecutionCertainty` recovery evidence and never become authoritative failure. After proven remote success, result/artifact acquisition and `PersistentCapacityUnavailable` carry only fieldless `AuthoritativeRemoteSuccess`; irrecoverable required-result loss terminates only as `ResultUnavailable` with fieldless `AuthoritativeRemoteSuccess`. Every missing, duplicate, or cross-combined recovery evidence and every other terminal-kind pairing is rejected, and no generated sequence invents a domain compensation-safety claim.

- **Done when:** `cargo test -p slingshot-development --test operation_and_job_properties` passes all named seed families and retained counterexamples with replay equivalence, universal 431-byte continuation-authority/validation precedence, no-partial configuration, truthful FileVault publication and parent-orderability state, terminal immutability, exact conditional recovery/result-unavailable evidence, idempotency, and scheduler fairness, and `scripts/quality` succeeds.
