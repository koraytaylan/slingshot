---
id: daemon-restart-reattachment
title: "Daemon Restart Reattachment"
workstream: "0034"
kind: task
depends_on:
  - finite-state-machine-retry-idempotency
gated: false
touches:
  - examples/finite-state-machine/daemon-restart.machine.json
  - crates/slingshot-development/tests/fixtures/finite-state-machine-daemon-restart/**
  - crates/slingshot-development/tests/finite_state_machine_daemon_restart.rs
status: done
merged_as: "e24d3572b421f80f0deda6944cdb80d7ceb9861b"
---
# Daemon Restart Reattachment

Prove a restarted target daemon reconciles the accepted author logical operation and fenced effect checkpoint while the original attached protocol call reconnects, with timeout retry only in the delayed control.

**Steps:**

1. Commit stable workflow-store namespace, exact compatibility identity and restart-drift cases, daemon termination checkpoints, fake-author logical/outbox/fence plus bounded physical-record snapshot progression, reconnect timing below and above the handler deadline, durable record expectations, nested acknowledgement shapes, protocol-child counts, and final FSM history before implementation.
2. Run the real FSM executor and terminate the target daemon after author acceptance and before terminal event persistence.
3. Restart the same profile/environment daemon against the same state directory while preserving FakeAuthor's one logical operation, effect checkpoint, and bounded known physical-record set.
4. Keep the original Slingshot protocol child alive and let Plan 0006/0007 reconnect/resubscribe plus Plan 0005 startup recovery reconcile the same digest-bound daemon/author logical operation and effect fence before the normal handler deadline. Every physical retry or duplicate that loses the gate no-ops.
5. Add a delayed-restart variant that exceeds the handler deadline and therefore may use the ordinary `timeout` retry with the same operation key.
6. Compare uninterrupted and both restarted `ack.result.structured` terminal results with absent `structured_sha256`, store namespace/operation key, daemon/author logical identifiers, fence/effect facts, bounded physical identifier sets, exact contract provenance, artifact state where applicable, protocol-child counts, and workflow history.

**Tests:**

- The restarted daemon discovers the nonterminal logical operation, exact provenance, winning fence/effect checkpoint, and bounded known physical identifiers from storage.
- Snapshot reconciliation reaches terminal state without requiring a replayed missing event.
- The within-deadline case uses one protocol child, emits no tool error, and receives the terminal result after transparent reconnect.
- The delayed case may use more than one child only through `timeout`, and every child uses the same operation key.
- FakeAuthor preserves exactly one durable logical operation and at most one fenced command-effect attempt across both daemon processes while permitting only the manifest-bounded duplicate physical records/attempts, all losers no-op.
- Uninterrupted and restarted runs preserve the same workflow namespace/key and produce equivalent terminal FSM state plus exact nested undigested command result.
- Every original and replacement Slingshot command-line, Model Context Protocol, and daemon child uses the shared Plan 0004 typed private test-root bootstrap; the hostile temporary production-root sentinel is untouched and absent from output, and no production root override is introduced.

- **Done when:** `SLINGSHOT_FINITE_STATE_MACHINE_EXECUTABLE=<finite-state-machine-executable> SLINGSHOT_EXECUTABLE=<slingshot-executable> cargo test -p slingshot-development --test finite_state_machine_daemon_restart` proves within-deadline daemon replacement is transparent to one protocol child and every timing variant reaches the equivalent terminal result with unchanged exact contract provenance, one durable logical operation, and at most one fenced command-effect attempt despite bounded no-op physical duplicates.
