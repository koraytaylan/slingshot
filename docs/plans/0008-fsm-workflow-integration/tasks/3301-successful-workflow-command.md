---
id: successful-workflow-command
title: "Successful Workflow Command"
workstream: "0033"
kind: task
depends_on:
  - workflow-handler-operation-keys
gated: false
touches:
  - examples/finite-state-machine/success.machine.json
  - crates/slingshot-development/tests/fixtures/finite-state-machine-success/**
  - crates/slingshot-development/tests/finite_state_machine_success.rs
status: planned
merged_as: ""
---
# Successful Workflow Command

Prove one real FSM effect calls Slingshot, waits through the target daemon, and reaches its declared success state with structured content.

**Steps:**

1. Commit the success machine, stable workflow-store namespace, handler inputs, exact `FiniteStateMachineCompatibilityIdentity`, independent daemon-runtime/author-transport drift failures, fake-author logical-operation/outbox/duplicate-physical-record script, expected canonical `MachineOutcomeEnvelope`, complete expected acknowledgement shape, and expected FSM history before implementation.
2. Define a machine that emits load_content with a bounded `workflow_effect_operation_key` and repository path, then advances only on the handler's declared success event.
3. Require the repository/sidecar/embedded/compatibility-manifest/`Hello`/capability contract identities to agree and record them before sending the starting event, then run the pinned fsm execute process through the expanded Model Context Protocol handler, Slingshot target daemon, and FakeAuthor. Every drift case stops with no operation, physical record, or effect.
4. Have FakeAuthor expose zero, one, or multiple transport-contract-bounded physical Sling records/attempts for one durable logical operation, permit only the winner of the linearizable `ExecutionNotStarted` to `ExecutionStarted` fence to attempt the effect, prove every loser no-ops, and emit queued, running, and succeeded observations with a bounded structured content result.
5. Query final instance state and history through the real FSM command line and compare canonical evidence, requiring `ack.result.structured` to be deeply and canonically equal to the byte-identical envelope below the pinned size cap and `ack.result.structured_sha256` to be absent.

**Tests:**

- The fake author records the exact load_content_as_json tool command, one durable logical operation, one successful fenced effect, and a bounded physical Sling record/attempt set whose losing consumers have no command effect.
- The target daemon records queued through succeeded monotonically.
- FSM records one successful acknowledgement whose `result.structured` equals the complete Slingshot envelope object, whose `result.structured_sha256` is absent, and fires only the configured success event.
- The final machine state and history match committed fixtures.
- Process captures contain no credential, publisher route, or unbounded content.
- Every Slingshot command-line, Model Context Protocol, and daemon child uses the shared Plan 0004 typed private test-root bootstrap; the hostile temporary production-root sentinel is untouched and absent from output, and no production root override is introduced.
- The scenario receipt and durable daemon/author logical records carry the exact compatibility identity; independently changed runtime/transport bytes, sidecar, embedded value, manifest value, `Hello`, capability, or terminal record provenance is rejected before execution or state advance without changing the exact envelope shape.

- **Done when:** `SLINGSHOT_FINITE_STATE_MACHINE_EXECUTABLE=<finite-state-machine-executable> SLINGSHOT_EXECUTABLE=<slingshot-executable> cargo test -p slingshot-development --test finite_state_machine_success` runs the exact-digest-gated real process chain and reaches the exact successful FSM state with one durable logical operation and at most one fenced command-effect attempt despite bounded no-op physical duplicates, `ack.result.structured` equal to the below-cap machine envelope, and no `structured_sha256`.
