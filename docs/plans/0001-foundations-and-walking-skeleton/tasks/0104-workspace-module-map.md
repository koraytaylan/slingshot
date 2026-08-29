---
id: workspace-module-map
title: "Workspace Module Map"
workstream: "0001"
kind: task
depends_on:
  - workspace-scaffold
gated: false
touches:
  - crates/slingshot-domain/src/lib.rs
  - crates/slingshot-configuration/src/lib.rs
  - crates/slingshot-agent-protocol/src/lib.rs
  - crates/slingshot-local-protocol/src/lib.rs
  - crates/slingshot-agent-connection/src/lib.rs
  - crates/slingshot-storage/src/lib.rs
  - crates/slingshot-daemon/src/lib.rs
  - crates/slingshot-command-line/src/lib.rs
  - crates/slingshot-test-support/src/lib.rs
  - crates/slingshot-development/src/lib.rs
  - crates/slingshot-domain/src/command/mod.rs
  - crates/slingshot-domain/src/remote_job.rs
  - crates/slingshot-agent-protocol/src/remote_job.rs
  - crates/slingshot-agent-connection/src/authentication/mod.rs
  - crates/slingshot-configuration/src/testing/mod.rs
  - crates/slingshot-storage/src/operation/mod.rs
  - crates/slingshot-daemon/src/operation/mod.rs
  - crates/slingshot-daemon/src/platform_runtime/mod.rs
  - crates/slingshot-daemon/src/process_checkpoint.rs
  - crates/slingshot-daemon/src/runtime_namespace.rs
  - crates/slingshot-daemon/src/ownership.rs
  - crates/slingshot-daemon/src/local_server.rs
  - crates/slingshot-daemon/src/service.rs
  - crates/slingshot-command-line/src/commands/mod.rs
  - crates/slingshot-command-line/src/model_context_protocol/mod.rs
  - crates/slingshot-command-line/src/platform_runtime/mod.rs
  - crates/slingshot-command-line/src/explicit_daemon_start.rs
  - crates/slingshot-command-line/src/daemon_connection.rs
  - crates/slingshot-command-line/src/command_line.rs
  - crates/slingshot-command-line/src/daemon_entry.rs
  - crates/slingshot-test-support/src/platform_runtime/mod.rs
  - crates/slingshot-test-support/src/daemon_process.rs
  - crates/slingshot-test-support/src/finite_state_machine_executable.rs
  - crates/slingshot-test-support/src/process_harness.rs
  - crates/slingshot-test-support/src/runtime_harness.rs
  - crates/slingshot-test-support/src/supervised_child.rs
  - crates/slingshot-development/src/daemon_chaos_subject.rs
  - crates/slingshot-development/src/dependency_direction.rs
  - crates/slingshot-development/src/platform_runtime_contract.rs
  - crates/slingshot-development/src/supported_platform_matrix.rs
  - crates/slingshot-development/src/source_policy.rs
  - crates/slingshot-development/src/rustsec_advisory_pin.rs
  - crates/slingshot-development/tests/workspace_module_map.rs
  - "crates/slingshot-development/tests/fixtures/workspace-module-map/**"
status: done
merged_as: "9e30c149f64762b0be1020b423eafe8ac2284fd0"
---
# Workspace Module Map

One checked module-ownership map gives Plan 0001 tasks and all later plans stable crate and module-family roots without preempting a later feature plan's exact leaf inventory.

**Steps:**

1. Author an independently ordered fixture mapping every crate, structural module-family root named by Plans 0001–0009, and exact Plan 0001 top-level source leaf in this task's footprint to its owning crate and architectural layer before adding module declarations. Do not enumerate a later plan's feature-owned leaf modules.
2. Adopt the ten exact scaffold-owned `lib.rs` files and create the documented standard-library-only family roots named in `touches`: domain command/durable-agent-job, agent-protocol conversion, authentication/testing/operation/command/Model Context Protocol/platform families, daemon process-checkpoint, development daemon-chaos-subject, and test-support daemon/process/runtime/path seams.
3. Declare and create documentation-only compiling shells for every exact Plan 0001 daemon, command-line, test-support, and development leaf named in this task's footprint. Leave the auto-discovered command-line and development `main.rs` entries to their behavior-owning tasks, the local-protocol inventory to `minimal-local-protocol`, platform-runtime family leaves to their platform owners, and every later plan leaf to that plan's module scaffold or dependency-ordered feature task. In particular, create no Plan 0003 command leaf inventory or command-specific source file; task `command-module-scaffold` owns that exact inventory after adopting the existing domain `command` root.
4. Keep every structural root and Plan 0001 leaf shell compilable with present-state documentation and no placeholder body, feature behavior, public constant, or external dependency.
5. Add a bidirectional structural assertion that rejects missing/additional/misowned roots or Plan 0001 leaves, later-plan feature leaves in the fixture, product roots placed in support crates, reusable process/path values placed in development, and any source/declaration/fixture/footprint mismatch.

**Tests:**

- The fixture, exact `touches` source list, and compiled module tree have the same complete ordered set and exact crate owner for every structural root and Plan 0001 leaf; a wildcard or later-plan feature-owned leaf path fails the assertion.
- `AgentJobIdentifier`, `AgentJobState`, `JobEventSequence`, and `EventStreamCursor` paths belong to domain; wire conversions belong to agent protocol; the process-checkpoint observer belongs to daemon; `FiniteStateMachineExecutable` and generic process harness paths belong to test support.
- Every structural module and Plan 0001 leaf shell passes documentation warnings and source policy without `todo!`, `unimplemented!`, placeholder panic, unsafe syntax, configured documentation marker, planning-only heading, behavior, or feature constant; the semantic present-state review checklist passes, and the domain command root contains no command-specific leaf declaration yet.

- **Done when:** `cargo test -p slingshot-development --test workspace_module_map && RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps` proves the complete structural crate/module-family tree and exact Plan 0001 leaf inventory each have one owner, compile as present-state structure, and leave every later feature-leaf inventory to its owning plan.
