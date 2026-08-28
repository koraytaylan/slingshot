---
id: resume-operation-recovery
title: "Resume Operation Recovery"
workstream: "0017"
kind: task
depends_on:
  - fair-bounded-operation-scheduler
  - list-operations
  - local-operation-envelopes
  - operation-wait-and-progress
gated: false
touches:
  - crates/slingshot-domain/src/operation.rs
  - crates/slingshot-storage/src/operation_repository.rs
  - crates/slingshot-daemon/src/operation_recovery.rs
  - crates/slingshot-daemon/tests/operation_recovery.rs
  - "crates/slingshot-daemon/tests/fixtures/operation-recovery/**"
status: planned
merged_as: ""
---
# Resume Operation Recovery

An operation paused in a manually resumable nonterminal recovery-required fact needs one explicit, compare-and-set path back to scheduler eligibility without creating or directly submitting replacement work.

**Steps:**

1. Author current-target, exact-selected-revision, changed-metascope, changed `VerifiedIdentityManagementTrustPolicyIdentity`, changed `VerifiedAuthorTrustPolicyIdentity`, exact-precondition, repeated, concurrent, stale-operation-revision, changed-category, multiple recovery cycles, later-progress, terminal, receipt-boundary, maintenance-removal and maintenance/replay race, wrong-target, restart, and status/list/wait fixtures before the service.
2. Accept the versioned `ResumeOperationRecovery` request only for the current target partition and selected-environment revision, with the existing operation identifier, expected operation revision, and expected recovery category as mandatory preconditions.
3. Define a closed domain recovery-resume fact and a bounded target/selected-revision/operation/source-keyed `RecoveryResumeReceipt`. After current daemon target/revision validation, query the receipt and truthful current operation status in one repository snapshot before current-state validation; only an exact selected-revision/source-operation-revision/category/fingerprint returns replay plus that status even after later progress, another recovery cycle, or terminal settlement. Linearize concurrent maintenance wholly before that snapshot or after it so receipt replay never observes a receipt without its owning status.
4. For a source without a receipt, add one repository compare-and-set that verifies the operation remains nonterminal and its exact recovery category is explicitly manual-resume eligible, preserves the immutable command fingerprint, records scheduler eligibility plus the receipt, and increments the existing operation revision exactly once.
5. Keep the existing operation, command, installation, target, and remote-job identities unchanged. The service allocates no identifier, invokes no executor, submits no remote work, and only makes the durable row eligible for the scheduler.
6. Publish the committed revision to waiters and prove status/list expose the same recovery-resume fact; reopening reconstructs every bounded receipt plus the eligible row and lets the scheduler select each applied revision at most once through its ordinary revision guard. Terminal maintenance removes receipts only with their owning terminal operation.

**Tests:**

- Exact current-target preconditions commit one eligibility revision and receipt, and concurrent identical calls return one applied plus deterministic replay responses.
- Exact repeats after later progress, a later recovery cycle, terminal settlement, and process restart replay the original receipt without another revision/schedule; a maintenance race returns either the complete replay/status snapshot or the post-removal missing result, never a torn receipt; changed metascope, `VerifiedIdentityManagementTrustPolicyIdentity`, or `VerifiedAuthorTrustPolicyIdentity` and fresh stale, wrong-category, non-paused, terminal, or wrong-target requests leave every repository byte unchanged.
- Applied and replayed requests preserve the original command fingerprint and every existing local/remote identity byte-for-byte.
- The resume service records no new operation/job/resume identity and makes zero executor or transport calls.
- Storage persists bounded domain recovery-resume receipts and gains no local-protocol dependency; exact replay consumes no additional receipt capacity, and one fresh source above the named limit refuses unchanged.
- Status, bounded list, and every attached wait observe the same persisted resume revision; a disconnected waiter changes nothing.
- Restart before scheduler selection reconstructs one eligible row, while restart after selection cannot schedule the same revision twice.

- **Done when:** `cargo test -p slingshot-daemon --test operation_recovery` proves target-and-selected-revision-bound exact-precondition recovery resumption commits once, durably replays every exact source after later/terminal/restart state until maintenance, rejects changed metascope, `VerifiedIdentityManagementTrustPolicyIdentity`, or `VerifiedAuthorTrustPolicyIdentity`, bounds distinct receipts, preserves conflicts, and performs zero direct execution, and all workspace gates succeed.
