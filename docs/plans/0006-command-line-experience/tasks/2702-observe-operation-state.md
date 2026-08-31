---
id: observe-operation-state
title: "Observe Operation State"
workstream: "0027"
kind: task
depends_on:
  - submit-and-detach-operations
gated: false
touches:
  - crates/slingshot-command-line/src/artifact_download.rs
  - crates/slingshot-command-line/src/artifact_staging_lock.rs
  - crates/slingshot-command-line/src/artifact_staging_metadata.rs
  - crates/slingshot-command-line/src/operation_observation.rs
  - crates/slingshot-command-line/tests/operation_observation.rs
  - "crates/slingshot-command-line/tests/fixtures/operation-observation/**"
status: done
merged_as: ""
---
# Observe Operation State

Implement status, wait, explicit recovery resume, result, and the shared verified publication engine used by operation-artifact and operation-free maintenance-result retrieval over the local protocol while preserving durable daemon ownership and every pre-existing destination.

**Steps:**

1. Commit transcripts for current/historical target partitions; exact/mismatched `DaemonRuntimeContractDigest`; exact/missing/mismatched current `AuthorAgentTransportContractDigest`; reused operation identifiers; each operation state; both legal `RecoveryExecutionEvidence` branches and every illegal cross-combination; post-success `PersistentCapacityUnavailable`; reconnecting wait; current-target `RecoveryRequired` resume with exact/replayed/stale/missing expected revisions and exact/changed/missing recovery categories; exact receipt replay after later progress/another recovery/terminal/restart; every terminal result including `ResultUnavailable` with fieldless `AuthoritativeRemoteSuccess`; artifact-identifier chunks, missing data, digest mismatch, destination/staging symlinks, nonregular files, unsafe modes, existing destination collision, synchronization failure, replacement race, two-process collision, crash-then-resume; and interruption immediately before/at/after atomic publication and before/during/after final success rendering.
2. Send exact expected `DaemonRuntimeContractDigest` on every versioned observation request. Permit a validated historical author-target digest only for terminal status/result/artifact reads; default to current, include the digest in every outcome, and reject historical wait/resume/nonterminal access. Require exact installed `AuthorAgentTransportContractDigest` before interpreting any current agent-backed status/result/failure/artifact fact; retain a different historical digest only as opaque read-only provenance, never current compatibility.
3. Implement current-target `operation resume` through Plan 0004's `ResumeOperationRecovery`, require the caller's observed revision and exact observed recovery category, and return the truthful `operation_status` envelope with applied or replayed durable receipt plus the unchanged conditional recovery evidence. Prove exact fresh preconditions schedule one manually resumable `RecoveryRequired` fact without converting `ExecutionCertainty` to `AuthoritativeRemoteSuccess` or the reverse, an exact committed source succeeds as replay without another schedule even after later/terminal/restart state, and active, stale-revision, changed-category, missing, historical, or receipt-limit-exhausted fresh requests schedule nothing.
4. Derive deterministic same-directory staging, sidecar, and lock names from target profile/environment and author-target identity digest plus the closed payload identity: operation identifier and artifact identifier for an operation artifact, or `MaintenanceResultIdentifier` with no operation/slot for a maintenance result. Acquire an exclusive cross-platform lock before reading either stage file and hold it through complete final output acknowledgement.
5. Open stage and sidecar with no-follow semantics as current-user-only regular files and retain the opened handles; canonically persist target/revision, closed operation-artifact or maintenance-result identity, digest, total length, verified length, and a closed transfer/ready-to-publish/published-receipt state. Resume stale unlocked transfer state only when those facts and same-handle length agree; the two identity shapes are disjoint and never default a missing operation.
6. After bounded chunks verify complete length/digest, synchronize the staging handle and containing directory as required and synchronize the exact ready-to-publish receipt. Make the successful non-overwriting atomic publication syscall the success linearization point. An interrupt that wins before it publishes no new destination and leaves resumable private state; a signal at/after it cannot select transfer interruption or attempt a second publication.
7. Retain the exact receipt and lock through the final success-output acknowledgement. Remove private receipt/staging state only after that renderer commit. On process/output loss after publication, let an identical invocation accept only the exact ready/published receipt plus a no-follow same-handle regular destination matching every recorded identity, length, and digest; then skip download/publication and re-render the original success. A missing or mismatched fact is an ordinary collision and preserves the destination.
8. Add deterministic disconnect/reconnect, concurrent-process, crash/restart, path-replacement, and filesystem failure tests around every durable boundary, including the gap after publication but before published-state synchronization, proving any unrelated existing destination or destination symlink remains untouched.

**Tests:**

- `operation_observation` pins status/wait/result behavior across all states and reconnect points; every recovery status contains exactly one legal evidence branch, post-success capacity/result/artifact acquisition uses fieldless `AuthoritativeRemoteSuccess`, and terminal `ResultUnavailable` is the only failure paired with fieldless `AuthoritativeRemoteSuccess`. Recovery resume compares exact observed revision/category, preserves that evidence, returns the applied or durable replay receipt with truthful current status after later/terminal/restart state, and schedules exactly once only from a fresh manually resumable source.
- Operation-artifact and operation-free maintenance-result cases prove a second saver fails without stage access, a crashed owner's unlocked matching transfer resumes, repeated chunks are harmless, and no partial, unsynchronized, digest-mismatched, or identity-mismatched file reaches the destination. Independent race cases prove pre-publication interruption publishes nothing, publication wins at/after its atomic commit, exact receipt recovery re-renders success without transfer/collision after incomplete output, and unrelated existing destinations/symlinks remain preserved.

- **Done when:** `cargo test -p slingshot-command-line --test operation_observation` proves current-target expected-revision/category recovery resume applies once from manually resumable recovery, preserves exactly one conditional recovery-evidence branch, and durably replays exact committed sources after later/terminal/restart state without rescheduling; terminal result-unavailable evidence is exact, target-qualified reads are unambiguous, and the exclusive locked publication engine keeps operation-artifact and operation-free maintenance-result identities disjoint, makes atomic publication the one success commit, preserves pre-publication resumability, and authenticates post-publication receipt re-rendering without collision or republishing.
