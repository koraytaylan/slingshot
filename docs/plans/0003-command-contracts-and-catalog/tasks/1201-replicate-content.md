---
id: replicate-content
title: "Replicate Content"
workstream: "0012"
kind: task
depends_on:
  - command-module-scaffold
  - repository-path
gated: false
touches:
  - crates/slingshot-domain/src/command/replicate_content.rs
  - crates/slingshot-domain/tests/fixtures/commands/replicate_content/**
  - crates/slingshot-domain/tests/replicate_content.rs
status: done
merged_as: "e8c0b7d3a9212f0e4e3ad02f9c29494ef46ed610"
---
# Replicate Content

Represent author-side replication of one repository path with an explicit recursive choice and no direct publisher connection.

**Steps:**

1. Commit canonical command/result fixtures and language-neutral agent conformance scenarios for recursive/nonrecursive preflight; missing/denied input; candidate-count/cooperative traversal-duration bounds; pre/post traversal-call late return and cancellation; sorted frozen manifest under concurrent additions; all-accepted success; zero/positive partial rejection; pre-InFlight admission-duration/cancellation; post-InFlight late return/cancellation ambiguity; physical retry/restart checkpoints; and publisher-delivery nonclaim before implementation.
2. Implement ReplicateContentCommand with RepositoryPath and an explicit recursive Boolean that is never inferred from omission.
3. Define bounded no-effect preflight and a complete immutable ascending candidate manifest before admission; implement success only after every distinct path is admitted to the author replication service with source path and accepted item count.
4. Keep publisher endpoints and credentials out of the command and result; the author-hosted job owns replication.
5. Register only closed `source_not_found`, `source_access_denied`, `candidate_limit_exceeded`, `traversal_budget_exceeded`, `admission_rejected`, `admission_budget_exceeded`, and `admission_outcome_unknown` failures with the exact source-path or accepted/remaining/current-path fields and zero/partial/unknown disposition rules; expose write and non-intrinsically-idempotent classification through the later catalog.
6. Pin durable NotStarted/InFlight/Accepted per-path agent checkpoints and cooperative boundary order. Before InFlight, cancellation leaves the current path unoffered/NotStarted and duration expiry is unaccepted admission-budget failure. Immediately after a blocking replication call returns, check cancellation/time before interpreting its payload; any late return, cancellation, timeout, or interruption after InFlight is outcome unknown. A never-returning call is not preempted by the clock; its surviving InFlight checkpoint remains unknown. Physical retry resumes NotStarted, never reoffers Accepted, and never repeats surviving InFlight.
7. Supply request-context validation that requires every echoed source path to equal the command path, requires nonrecursive current paths to equal it, requires recursive current paths to be it or a validated descendant, and checks accepted/remaining counts against the named candidate bound before persistence.

**Tests:**

- Recursive and non-recursive requests have distinct canonical fixtures.
- Missing recursive, invalid path, unknown fields, and malformed result counts are rejected.
- Root and nested paths reuse RepositoryPath behavior.
- Result/failure counts stay within the named candidate bound and never claim publisher delivery; success count equals the complete manifest, while every admission failure count pair sums to it and includes current_path in remaining_item_count.
- Independently authored scenarios pin recursive inclusion, nonrecursive exclusion, preflight-before-effect, frozen deterministic manifest, duplicate elimination, budget/failure disposition, and admitted-count meaning; Rust validates their documents without running replication.
- Checkpoint/restart vectors require no Accepted or ambiguous InFlight path to be re-admitted and distinguish zero-effect, partial-effect, and unknown-outcome failure facts.
- Exact-deadline vectors distinguish pre-InFlight budget exhaustion from during-InFlight outcome ambiguity and never classify an ambiguous offer as unaccepted.
- Traversal cancellation/late-return vectors stop before admission and discard the manifest; admission vectors pin the exact NotStarted/InFlight/Accepted checkpoint and accepted prefix for cancellation before InFlight versus a call returned late or cancelled after InFlight. They prove call/checkpoint traces and no re-admission, not hard interruption of a blocking JCR or replication-library call.
- Request and result fixtures contain no publisher address or credential field.
- Structurally valid success/failure documents copied from a different source command, carrying an out-of-scope current path, or impossible bounded count relationship fail request-context validation before persistence.

- **Done when:** cargo test -p slingshot-domain --test replicate_content validates path/result/failure/closed-shape/publisher-isolation invariants and the complete bounded cooperative traversal/late-return/cancellation/manifest/admission/checkpoint agent-conformance inventory without running or claiming to preempt replication.
