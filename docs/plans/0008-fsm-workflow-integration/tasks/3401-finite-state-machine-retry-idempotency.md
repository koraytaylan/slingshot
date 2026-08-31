---
id: finite-state-machine-retry-idempotency
title: "FSM Retry Idempotency"
workstream: "0034"
kind: task
depends_on:
  - successful-workflow-command
  - failed-workflow-command
gated: false
touches:
  - examples/finite-state-machine/retry.machine.json
  - crates/slingshot-development/tests/fixtures/finite-state-machine-retry/**
  - crates/slingshot-development/tests/finite_state_machine_retry_idempotency.rs
status: done
merged_as: "ffd3f8d4a6603380ea957221471cef4b37e1ecd5"
---
# FSM Retry Idempotency

Prove retry attempts for one workflow effect occurrence attach to one durable logical operation and effect gate while a second deliberate occurrence of the same handler uses a different logical operation, without asserting physical Sling exactly-once delivery.

**Steps:**

1. Commit machines with one stable workflow-store namespace, exact compatibility identity and drift cases, two intended occurrences of the same handler, distinct bounded `workflow_effect_operation_key` values, `retry.on = ["timeout"]` for the first occurrence, delayed terminal fake-author logical-operation/outbox/fence behavior including bounded duplicate physical records, an externally broken-child control, and two independent stores sharing an instance request identifier under distinct namespaces, plus expected attempts/logical operations/effect gates/physical sets and final histories.
2. Configure a named finite FSM handler timeout and deterministic backoff; the Slingshot tool has no wait-time argument and waits for authoritative terminal state.
3. After exact contract-provenance preflight, let FakeAuthor accept the first logical operation and delay terminal state past the first handler timeout, causing the pinned FSM to terminate the protocol child, classify `timeout`, and detach that child without cancelling the operation or effect gate.
4. Let the real FSM executor start a later protocol child with byte-identical arguments and the same store-namespaced occurrence key, attach to the existing daemon and author logical operation, and receive the eventual terminal success at `ack.result.structured` with no digest fallback. Permit the transport-bounded duplicate physical record/attempt set, but require at most one winning `ExecutionStarted` effect attempt and no-op losers.
5. Emit the second intended occurrence through the same handler with its distinct key and complete its distinct daemon/author logical operation and effect gate without asserting one physical record.
6. In a separate control, externally terminate a protocol child, pin the executor's actual non-timeout classification, exclude it from retry, and prove the durable operation continues without cancellation.
7. Run equal instance request identifiers/occurrences in two independently namespaced store roots and prove their canonical operation keys, daemon/author logical operations, and effect gates are distinct; restart one root and prove its namespace/key remains exact. Physical record sets are not compared as identity.
8. Compare FSM attempt records, exact protocol sequences, Slingshot operation records, fake-author logical/outbox/fence and bounded physical-record facts, exact contract provenance, namespaces, and final workflow history.

**Tests:**

- FSM records the first occurrence's `timeout`, later successful exact envelope at `ack.result.structured`, and the second occurrence's successful nested acknowledgement; no conforming result has `structured_sha256`, and no nonterminal Slingshot result is mapped to `mcp_error`.
- The retry protocol children perform initialize, initialized, then direct tools/call with byte-identical arguments; the second intended occurrence differs only by its workflow-effect operation key and occurrence data.
- The target daemon and FakeAuthor contain exactly two durable logical operations: one for the retried occurrence and one for the second deliberate occurrence; each has at most one fenced command-effect attempt.
- FakeAuthor records retry attachment plus a manifest-bounded physical record/attempt set for each logical operation; all losing consumers no-op, and retry creates neither a third logical operation nor a second effect.
- The externally broken-child control records its actual non-retryable classification, starts no automatic retry, and leaves its accepted operation active.
- Equal instance request identifiers in distinct stable store namespaces create distinct keys/logical operations/effect gates; retry and restart inside one namespace preserve its exact key and provenance.
- The final workflow state/history matches the committed two-occurrence and two-store fixtures after accounting for the retry attempt.
- Every Slingshot command-line, Model Context Protocol, and daemon child uses the shared Plan 0004 typed private test-root bootstrap; the hostile temporary production-root sentinel is untouched and absent from output, and no production root override is introduced.

- **Done when:** `SLINGSHOT_FINITE_STATE_MACHINE_EXECUTABLE=<finite-state-machine-executable> SLINGSHOT_EXECUTABLE=<slingshot-executable> cargo test -p slingshot-development --test finite_state_machine_retry_idempotency` proves a real FSM `timeout` retry and store restart reuse one exact-provenance namespaced logical operation and at-most-one effect gate despite bounded no-op physical duplicates, deliberate occurrences and independent namespaces remain distinct, exact envelopes stay nested/undigested, and external child loss detaches without retry or cancellation.
