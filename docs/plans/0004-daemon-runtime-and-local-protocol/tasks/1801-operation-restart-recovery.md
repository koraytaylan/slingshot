---
id: operation-restart-recovery
title: "Operation Restart Recovery"
workstream: "0018"
kind: task
depends_on:
  - graceful-daemon-stop
gated: false
touches:
  - crates/slingshot-test-support/src/operation_fault_injection.rs
  - crates/slingshot-development/src/test_daemon_faults.rs
  - crates/slingshot-development/tests/operation_restart_recovery.rs
status: done
merged_as: "869561719f86555960fe639cd296c939f5216d90"
---
# Operation Restart Recovery

Durability is meaningful only if a new daemon makes the correct decision with no executor or waiter memory from its predecessor. This task injects loss at each named operation boundary.

**Steps:**

1. Build generic process/fault controls in test support and helper checkpoints for capacity-counted admission, submitting, progress/conditional-recovery-evidence/retry observation, completed-result capacity refusal, recovery-resume receipt commit before/after scheduler selection and after later/terminal state, inline/external maintenance preview association, preview supersession, application-receipt/result-association commit/replay and prior-receipt/result retirement, artifact reservation/install/association, concurrent artifact in-place rewrite/truncation during transfer, terminal commit, pinned-reader/write churn at every header/frame-derived WAL threshold, transaction-page/frame-growth refusal, passive checkpoint, checkpoint deadline/backpressure, database/WAL/SHM/replacement physical ceilings, prohibited VFS/temp opens, SQLite invariant loss, restart recovery, and graceful truncate checkpoint.
2. At each checkpoint, terminate the existing development binary's internal test-daemon subcommand without graceful stop and start a fresh test-daemon process through that same binary with the same name-derived namespace and persistent state root.
3. Add fail-closed processes that crash at `initializing`/database/`registered`, remove/corrupt/mismatch global identity, create impossible ledger/database combinations, fail each cut of Plan 0002's committed-generation proof before a snapshot exists, or inject independently changed typed AuthorTargetIdentity, SelectedEnvironmentRevision, and both while old target/revision history contains terminal or each nonterminal state. Include profile-authentication-contract-only drift; separate restart-visible provider-policy-verified platform Identity-Management-Services-root and effective platform-plus-selected-additional-author-CA identity drift; hostile additional-author-CA Identity-Management-Services interception; and Plan 0002's named Basic and Cloud organization/client/`integration.id`-backed technical-account vector outputs. Contrast equal typed values under genuine same-principal rotation and canonical-equivalent dual-root policies without reconstructing upstream preimages or hashing the target rendering.
4. Record target-partitioned fake invocations, database/conditional-evidence/recovery/progress/maintenance-association revisions, artifact slots, operation-free maintenance-result identities/owners, installation bytes, readiness publication, injected UTC/monotonic observations, and final result.
5. Assert that capped nonterminal invocation may run again only through exact committed recovery resumption and ordinary scheduler selection, exact resume/apply receipts and their result associations replay after reopen without repeating effects, superseded previews and retired receipts are unreadable, terminal facts never invoke again, identity/target audit refusal changes no row, and no partial artifact becomes success.
6. Print seed/checkpoint on failure and accept a named iteration override for deeper local runs.

**Tests:**

- Every helper checkpoint reopens a valid database and reaches one coherent terminal operation or an explicitly retryable nonterminal state.
- No lifecycle revision is duplicated or regresses; exactly one terminal transition exists.
- A crash after terminal commit never invokes the executor again.
- Partial artifact installation never becomes a successful result, while a committed artifact always verifies after restart.
- Restart reconstructs operation/artifact counters and active reservations exactly; completed remote work blocked on capacity remains one recovery-required row and never resubmits.
- A pinned reader cannot outlive the manifest deadline; write churn reaches typed no-mutation backpressure at the independently calculated header/frame high-water before the WAL/SHM/database physical ceiling, and reopen either performs the bounded recovery checkpoint or refuses readiness byte-preservingly without accepting another write. Missing build/configuration/VFS/SQL-inventory evidence or any attempted ambient/unlisted disk transient also refuses readiness unchanged.
- Installation loss/corruption/mismatch, incomplete/mismatched committed generations, and old-target/revision nonterminal rows refuse before readiness, admission, and executor/network access byte-for-byte unchanged. This includes profile-contract, canonical-metascope, either distinct selected server-authentication root snapshot identity, or principal drift; genuine same-principal credential rotation remains compatible, live provider-policy/additional-author-CA edits do not alter the retained snapshot, restart-visible root-policy drift changes only revision, an additional author CA never authorizes Identity Management Services, and terminal old security-context history remains queryable with its stored revision-bound fingerprint.
- Restart reconstructs recovery deadlines within the original delay under forward/backward UTC changes; ambiguous exhausted work stays nonterminal, post-success local completion retains authoritative remote success without certainty, and terminal failure retains its exact authoritative or fail-closed disposition.
- Restart after recovery-resume commit preserves one eligible fact plus its source receipt, exact replay remains available after later/terminal state, and one applied revision cannot invoke twice.
- Every maintenance crash point reopens to either the complete pre-apply inline/associated manifest state or the complete applied state plus target-qualified receipt and exact inline/associated result; exact preview/apply replay never duplicates, deletes, or credits twice, and retirement removes owned result associations atomically.
- Crash before global identity publication permits one later empty-root creation, while crash after publication preserves the one identifier and registered-target ledger.
- Exact initializing-target crashes resume to registered; every impossible ledger/database combination remains byte-for-byte unchanged without readiness.

- **Done when:** `cargo test -p slingshot-development --test operation_restart_recovery` passes every operation, durable resume/maintenance receipt and operation-free result-association, SQLite, artifact, installation, and target-partition checkpoint with byte-preserving refusal or one coherent recoverable/terminal/applied fact, and all workspace gates succeed.
