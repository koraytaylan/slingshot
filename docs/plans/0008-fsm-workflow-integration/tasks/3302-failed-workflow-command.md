---
id: failed-workflow-command
title: "Failed Workflow Command"
workstream: "0033"
kind: task
depends_on:
  - workflow-handler-operation-keys
gated: false
touches:
  - examples/finite-state-machine/failure.machine.json
  - crates/slingshot-development/tests/fixtures/finite-state-machine-failure/**
  - crates/slingshot-development/tests/finite_state_machine_failure.rs
status: planned
merged_as: ""
---
# Failed Workflow Command

Prove a terminal author logical-operation failure becomes a truthfully dispositioned Model Context Protocol tool error, an FSM failed acknowledgement, and the machine's authority-neutral declared failure transition.

**Steps:**

1. Commit the failure machine, stable workflow-store namespace, exact compatibility identity and drift cases, recursive-replication handler inputs with a multiple-path manifest, fake-author logical-operation/outbox/duplicate-physical-record positive-prefix `admission_rejected` failure, bounded expected `operation_terminal_error` `MachineOutcomeEnvelope`, complete expected acknowledgement shape, and expected FSM history before implementation; require `AuthoritativeRemoteFailure` and forbid an operation-certainty field.
2. Define a machine that emits `replicate_content` recursively and advances to its generic failed terminal state only through the handler's authority-neutral configured failure event.
3. Complete the exact repository/sidecar/embedded/manifest/`Hello`/capability provenance gate before the starting event, then run the pinned real FSM executor through the Slingshot protocol child and target daemon; every drift case leaves no daemon operation, author logical operation, physical Sling record, or effect.
4. Have FakeAuthor accept one logical operation, optionally expose a bounded duplicate physical Sling record/attempt set, allow only one winning fence to attempt the effect while all losers no-op, admit a positive manifest prefix, and return the registered `admission_rejected` failure with exact positive accepted count, remaining count, and current path after progress.
5. Assert the Slingshot `isError: true` tool result, `ack.result.structured` deep/canonical equality to the exact below-cap envelope with absent `structured_sha256`, independently validated partial-effect facts and `AuthoritativeRemoteFailure` disposition without certainty, generic FSM failure class, configured failure event, and terminal machine state.

**Tests:**

- FakeAuthor records one operation identifier, one durable logical operation, at most one fenced command-effect attempt, and only a bounded physical Sling record/attempt set whose losing consumers no-op.
- Slingshot reports the failed operation with the registered replication category, positive accepted count, matching remaining count/current path, target profile/environment, author-target identity digest, operation identifier, state, and exact authoritative-remote-failure disposition with no certainty member.
- FSM maps the tool error to mcp_error and records one failed acknowledgement whose `result.structured` equals that envelope while `result.error` carries the tool-error class and `result.structured_sha256` is absent.
- Only the configured failure event advances the instance.
- Neither the `isError` flag, `mcp_error` class, static failure event, nor agent-selected label is asserted to carry authority; only Plan 0003's validated positive-prefix replication facts and retained daemon disposition prove this fixture's partial-effect remote terminal failure.
- The final state, history, error shape, redaction, and author-only route trace match fixtures.
- Every Slingshot command-line, Model Context Protocol, and daemon child uses the shared Plan 0004 typed private test-root bootstrap; the hostile temporary production-root sentinel is untouched and absent from output, and no production root override is introduced.
- Scenario and durable records agree with the exact runtime/transport compatibility identity; drift is rejected before execution or FSM advancement and never changes the registered failure or envelope shape.

- **Done when:** `SLINGSHOT_FINITE_STATE_MACHINE_EXECUTABLE=<finite-state-machine-executable> SLINGSHOT_EXECUTABLE=<slingshot-executable> cargo test -p slingshot-development --test finite_state_machine_failure` runs the real process chain, derives authoritative remote failure without invented certainty from a registered positive-prefix replication rejection at `ack.result.structured` with no digest fallback, and reaches the generic failed FSM state through an authority-neutral declared failure event.
