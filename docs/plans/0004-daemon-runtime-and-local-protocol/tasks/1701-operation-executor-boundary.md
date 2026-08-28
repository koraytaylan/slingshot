---
id: operation-executor-boundary
title: "Operation Executor Boundary"
workstream: "0017"
kind: task
depends_on:
  - operation-lifecycle
  - checksum-verified-artifact-store
gated: false
touches:
  - crates/slingshot-domain/src/operation_executor.rs
  - crates/slingshot-daemon/src/unavailable_operation_executor.rs
  - crates/slingshot-test-support/src/fake_operation_executor.rs
  - crates/slingshot-development/src/slingshot_test_daemon.rs
  - crates/slingshot-development/src/main.rs
  - crates/slingshot-development/tests/operation_executor_composition.rs
status: planned
merged_as: ""
---
# Operation Executor Boundary

The daemon needs an inward execution port without making the product claim remote work exists. This task composes an explicit unavailable product adapter and confines the scripted fake to an internal test-daemon subcommand of the existing outer development executable.

**Steps:**

1. Write composition fixtures first for the exact two-binary workspace target set, exhaustive development-command dispatch, product unavailable, success, every terminal failure kind/disposition combination including ResultUnavailable/AuthoritativeRemoteSuccess, each legal recovery evidence/category combination including exhausted unknowns and remote-success-pending completion, ordered progress, inline/structured/declared artifacts, delayed completion, cancellation, and target-partitioned recording.
2. Define `OperationExecutor` in `slingshot-domain` in terms of typed command, target-partitioned identity, progress/artifact ports, and closed success/terminal-failure/recovery-required outcome using shared conditional recovery-evidence/disposition types; exclude local frames, SQLite handles, storage implementations, and daemon state.
3. Implement `UnavailableOperationExecutor` in daemon and make it the only product-binary composition; it refuses before admission with the stable unavailable outcome.
4. Implement `FakeOperationExecutor` in test support with deterministic scripted steps and explicit clocks/release controls. Implement the test-daemon composition in the declared `slingshot-development::slingshot_test_daemon` library module, then extend the existing exhaustive `slingshot-development` binary dispatcher with an internal `test-daemon` subcommand. Preserve every prior command branch, reject unknown commands, and invoke the subcommand through the existing binary path plus arguments in generic process support.
5. Record author-target digest, operation identifier, attempt, and emitted facts so restart/idempotency tests distinguish replay from another execution.

**Tests:**

- The product executable always uses unavailable execution, creates no row, and contains no fake composition.
- Cargo metadata retains exactly the Plan 0001 `slingshot` and `slingshot-development` binary targets; `slingshot_test_daemon.rs` is a development-library module and cannot appear as a third target.
- The development dispatcher preserves every existing command, routes only `test-daemon` to the fake composition, and rejects every unknown command.
- Each helper script emits exact lifecycle/progress and success, truthful kind/disposition terminal failure, or conditional-evidence recovery-required facts without a generic compensation-safety claim.
- Artifact scripts write through the real artifact-store interface and return verified metadata only.
- Dropped progress consumers do not block or fail executor completion.
- Invocation recording distinguishes replay without execution from a second execution.
- Cargo metadata proves product crates have no normal/build edge to test support or development, and only the outermost development library plus its existing process entry compose daemon with the fake.

- **Done when:** `cargo test -p slingshot-development --test operation_executor_composition` proves unavailable product composition, fake composition only through the existing development binary's internal `test-daemon` subcommand, preservation of its exhaustive dispatcher, exactly two workspace binaries, every closed executor outcome, bounded progress, verified result artifacts, and target-partitioned invocation counts, and all workspace gates succeed.
