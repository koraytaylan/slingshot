---
id: author-agent-operation-executor
title: "Author Agent Operation Executor"
workstream: "0023"
kind: task
depends_on:
  - artifact-download
  - durable-idempotent-submission
  - recovery-and-event-supervisor
gated: false
touches:
  - crates/slingshot-daemon/src/author_agent_operation_executor.rs
  - crates/slingshot-daemon/src/startup.rs
  - crates/slingshot-daemon/src/lib.rs
  - crates/slingshot-daemon/tests/author_agent_operation_executor.rs
  - crates/slingshot-daemon/tests/operation_submission.rs
  - crates/slingshot-development/tests/operation_executor_composition.rs
  - crates/slingshot-development/tests/operation_submission_process.rs
  - crates/slingshot-development/tests/concurrent_daemon_clients.rs
  - crates/slingshot-development/tests/local_operation_session.rs
  - "crates/slingshot-development/tests/fixtures/local-operation-session/**"
  - README.md
  - ARCHITECTURE.md
  - docs/DAEMON.md
  - crates/slingshot-development/tests/product_documentation.rs
status: done
merged_as: ""
---
# Author Agent Operation Executor

The production daemon needs one concrete composition that replaces the Plan 0004 fake boundary with authenticated author submission, filtered target-partitioned event supervision, durable live recovery, bounded machine result disposition, and verified artifacts.

**Steps:**

1. Write composition and present-state documentation assertions first for AuthorAgentTransportContractDigest, separate canonical-contract/dual-schema-annotation provenance, Plan 0002's opaque typed target/revision/provider and selected TLS 1.2/1.3/exact immutable `VerifiedIdentityManagementTrustPolicyIdentity`/`VerifiedAuthorTrustPolicyIdentity` route policies with no reload, merge, or cross-use, exact committed-generation refusal before admission, direct no-second-hash `AuthorTargetIdentityDigest`, equal/changed supplied target/revision including profile-contract/either named trust identity/principal vectors, hostile additional-author-CA interception of Identity Management Services, universal cluster-capable continuation-authority readiness, logical-operation/physical-attempt/fenced execution, Cloud lease refusal, author protection, unchanged-five-field compatibility, raw-canonical-before-Draft-before-typed input/result validation, load threshold, complete maintenance metadata/read/ownership branches, every result/failure/artifact outcome, recovery, shutdown, explicit fake injection, and superseded Plan 0004 unavailable-product fixtures.
2. Implement AuthorAgentOperationExecutor over the immutable selected environment, author-only authentication provider, capability cache, authenticated connection, target-partitioned RecoveryAndEventSupervisor, AgentJobRepository, and artifact store.
3. Install that executor in the production daemon startup path after configuration and storage open; keep `FakeOperationExecutor` available only through an explicit test constructor and never select it from production configuration. In the same atomic change, replace every Plan 0004 product-unavailable assertion and golden fixture in `operation_executor_composition`, daemon and process `operation_submission`, `concurrent_daemon_clients`, and `local_operation_session`, then update `README.md`, `ARCHITECTURE.md`, `docs/DAEMON.md`, and their executable assertions so all product surfaces describe the installed author-backed executor and its current failure/availability boundaries. Retain explicit injected `UnavailableOperationExecutor` unit coverage without representing it as production composition.
4. Audit every stored nonterminal row before binding or readiness. Compare the exact opaque typed target and SelectedEnvironmentRevision, both runtime/transport digests, canonical-contract digest plus both schema-root annotations/role digests, generation/derived identity, raw canonical command bytes, unchanged-five-field identity, SubmittedCommandDigest, and logical-operation/outbox/fence facts. Any mismatch refuses byte-preservingly before provider/executor/network/bind/readiness. Plan 0002 committed-generation failure yields no candidate snapshot and no admission. Equal supplied target/revision under genuine same-principal rotation remains compatible; profile-contract, principal, or selected-revision change refuses old nonterminal work without this task reconstructing upstream preimages or rehashing the target rendering.
5. Own exactly one RecoveryAndEventSupervisor and filtered DaemonSubscriptionIdentifier per AuthorTargetIdentity partition, attach every active operation by AgentOperationIdentifier, and detach on shutdown without sending job cancellation.
6. Keep readiness diagnostics bounded when the selected target's author is unavailable or its capacity is exhausted, but never represent mismatched durable target work as a ready daemon and never replay it into another partition.

**Tests:**

- Production composition invokes capabilities, artifact/annotation/digest-bound submission, stream/snapshot reconciliation, bounded raw-before-Draft-before-typed conversion plus Plan 0003 `validate_result_for_command`, and artifact completion through the selected-snapshot author-only client.
- Machine-inline and local stable-slot externalized results fit complete Plan 0004 and Model Context Protocol envelope budgets; remote JSON/package artifacts contain no inline bytes.
- Multiple operations share one AuthorTargetIdentity-partition subscription supervisor while retaining independent AgentOperationIdentifier, job sequences, and progress.
- Live and restart recovery register every durable retry decision before new admission; FakeAuthor may record bounded duplicate physical jobs, but only one logical operation and one fenced effect attempt exist.
- A proven remote success with unavailable local artifact capacity remains nonterminal with AuthoritativeRemoteSuccess and no result, resumes acquisition only after maintenance and exact recovery resume, never reaches submission again, and settles authoritative retention loss as ResultUnavailable without execution uncertainty.
- A committed-generation, target, or revision mismatch—including profile-contract, same-source principal, or restart-visible drift of `VerifiedIdentityManagementTrustPolicyIdentity` or `VerifiedAuthorTrustPolicyIdentity`—preserves every storage byte and produces zero provider/executor/outbound requests, endpoint binds, readiness signals, or operation admission. Same-principal secret rotation can recover the original partition; live root-source edits leave both clients unchanged; publisher, ambient-proxy, and additional-author-CA-only Identity Management Services canaries each receive zero connections.
- The production entrypoint contains no path that constructs the fake executor; explicit fake and unavailable test injection remains deterministic, while every superseded Plan 0004 product-default assertion now expects author-backed composition in the same change.
- Product documentation and its executable examples describe the author-backed production executor, its exact current routes and recovery behavior, and no unavailable-executor claim remains after this task.

- **Done when:** `cargo test -p slingshot-daemon --test author_agent_operation_executor`, `cargo test -p slingshot-daemon --test operation_submission`, `cargo test -p slingshot-development --test operation_executor_composition`, `cargo test -p slingshot-development --test operation_submission_process`, `cargo test -p slingshot-development --test concurrent_daemon_clients`, `cargo test -p slingshot-development --test local_operation_session`, and `cargo test -p slingshot-development --test product_documentation` prove production startup and every product process/golden assertion atomically install and document the author-backed executor and target-partitioned recovery supervisor, preserve same-principal rotation, refuse changed-principal or other mismatched durable work before bind without mutation/network access, reject unusably short Cloud leases without author traffic or refresh loops, share one filtered subscription, perform live/restart recovery, complete machine-inline/local/remote-artifact outcomes including operation-free maintenance metadata/read within both presentation budgets, isolate AuthorTargetIdentity partitions, and never dial publisher/proxy or implicitly cancel a Sling Job.
