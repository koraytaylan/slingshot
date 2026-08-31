---
id: operation-status-and-result
title: "Operation Status And Result"
workstream: "0017"
kind: task
depends_on:
  - execute-operation-service
gated: false
touches:
  - crates/slingshot-daemon/src/operation_queries.rs
  - crates/slingshot-daemon/tests/operation_queries.rs
status: done
merged_as: ""
---
# Operation Status And Result

Clients need stable point-in-time reads that never infer state from executor memory and never return unchecked artifact material.

**Steps:**

1. Write query tests first for every lifecycle state, each recovery evidence/category and resume-receipt state, every valid/invalid conditional failure kind/disposition payload combination including ResultUnavailable/AuthoritativeRemoteSuccess, target partitions sharing an identifier, missing, inline/structured/declared artifacts, corruption, and reopen.
2. Require target digest plus operation identifier and return status with target/revision provenance, lifecycle/revision, nonterminal conditional recovery execution evidence, bounded progress/recovery including current resume source/eligibility, truthful conditional terminal disposition when present, and workflow correlation.
3. Implement result so nonterminal operations return typed pending/recovery with exactly ExecutionCertainty or AuthoritativeRemoteSuccess; terminal failure returns kind plus exactly authoritative-nonexecution with `ConfirmedNotExecuted`, authoritative-remote-failure without certainty, ResultUnavailable with authoritative-remote-success without certainty, or fail-closed-indeterminate with one unknown certainty and bounded metadata; and success returns stored result/artifacts.
4. Resolve every artifact by `ArtifactIdentifier` and deterministic slot through checksum/length verification; map missing or corrupt bytes to explicit result errors without changing terminal operation facts.

**Tests:**

- Every lifecycle state returns exact persisted state/revision; recovery returns its legal conditional evidence and failed returns exactly its legal conditional disposition payload without client inference, invented certainty, or a generic compensation-safety claim.
- Missing operations are distinct from nonterminal results, and lookup never crosses target partitions.
- Inline results obey the machine budget; canonical structured-result artifacts use `application/json`, deterministic `structured_result`, and no remote URL.
- Valid terminal artifacts verify before access; missing and corrupt artifacts never return successful bytes.
- Reopening the daemon-facing repository yields identical status and result responses.

- **Done when:** `cargo test -p slingshot-daemon --test operation_queries` proves target-partitioned point reads, conditional terminal disposition reporting without invented certainty or compensation overclaim, bounded progress/recovery-resume facts, inline-or-canonical results, and mandatory artifact verification across reopen, and all workspace gates succeed.
