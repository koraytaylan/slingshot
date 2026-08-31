---
id: daemon-process-chaos
title: "Daemon Process Chaos"
workstream: "0036"
kind: task
depends_on:
  - operation-and-job-state-properties
  - pinned-coverage-fuzzing-tool
gated: false
touches:
  - crates/slingshot-daemon/src/process_checkpoint.rs
  - crates/slingshot-daemon/src/startup.rs
  - crates/slingshot-daemon/src/local_server.rs
  - crates/slingshot-daemon/src/operation_submission.rs
  - crates/slingshot-daemon/src/operation_scheduler.rs
  - crates/slingshot-daemon/src/author_agent_operation_executor.rs
  - crates/slingshot-daemon/src/operation/remote_submission.rs
  - crates/slingshot-daemon/src/operation/recovery_and_event_supervisor.rs
  - crates/slingshot-daemon/src/operation/artifact_completion.rs
  - crates/slingshot-daemon/src/operation/remote_result_settlement.rs
  - crates/slingshot-daemon/src/operation_wait.rs
  - crates/slingshot-daemon/src/operation_maintenance.rs
  - crates/slingshot-daemon/src/diagnostics.rs
  - crates/slingshot-daemon/src/shutdown.rs
  - crates/slingshot-test-support/src/daemon_fault_checkpoints.rs
  - crates/slingshot-test-support/src/lib.rs
  - crates/slingshot-development/src/daemon_chaos_subject.rs
  - crates/slingshot-development/src/main.rs
  - crates/slingshot-development/tests/daemon_process_chaos.rs
status: done
merged_as: ""
---
# Daemon Process Chaos

The daemon crosses local persistence, remote submission, event, artifact, waiter, and shutdown boundaries. A dedicated process composition terminates the production daemon application at each observable internal boundary and verifies what its successor can reconstruct.

**Steps:**

1. Define a daemon-owned `ProcessCheckpointObserver` with the closed checkpoint/phase vocabulary and an inactive production implementation. Call it at the exact installation-identity creation, runtime/transport-contract authentication, readiness, admission, remote-child creation, logical reservation, outbox prepare, Sling JobManager call/observation, worker lease/fence/no-return checkpoint, retry scheduling, author acceptance, event receipt, subscription-cursor/event persistence, snapshot-watermark reset, artifact prefix authentication/streaming, artifact installation, terminal commit, waiter delivery, maintenance, diagnostics rotation, and stop boundaries. The product command-line composition always injects the inactive implementation and exposes no configuration, process-environment, or command-line switch for another observer.
2. Commit named checkpoint/phase schedules and seeds first. Add the internal `daemon-chaos-subject` subcommand to the existing `slingshot-development` repository-command binary and compose the same production daemon application, storage, author executor, local server, and shutdown path there; only this development subcommand injects a control-channel observer that reports and flushes the exact checkpoint fact, then blocks before the next side effect. It creates no additional Cargo binary target.
3. Launch that existing repository-command executable with the internal subcommand, fake identity service, and fake author in temporary roots; wait for the inherited bounded control channel to report the selected checkpoint, verify its operation/target identity, and terminate the process through its operating-system handle without releasing the observer or requesting graceful cleanup.
4. Start a fresh subject owner with the same durable/runtime roots, resume to convergence, and collect installation and target identity, database history, retry schedule, artifact state, fake-author job count, executor invocation count, waiter output, diagnostics bounds, and runtime ownership evidence.
5. Authenticate exact `slingshot.daemon-runtime-contract/1` and `slingshot.author-agent-transport-contract/1` bytes/digests on initial start and reopen. Assert one cluster-wide logical operation, a bounded sorted set of zero/one/multiple physical Sling Job records, and at most one fenced command-effect attempt rather than inferring physical exactly-once delivery from one final local row.
6. Repeat startup with a changed selected-target revision and exercise an incompatible operation-protocol client through retained status and stop; mismatch must fail before readiness without mutating nonterminal state.
7. Print seed/checkpoint/phase evidence and support a named iteration override whose committed default is justified by measured continuous-integration time.

**Tests:**

- Every checkpoint and both sides of each durable boundary are reached through the daemon-owned observer, reopen one valid runtime namespace, and yield one coherent durable operation history.
- A conforming fake author preserves one logical operation across all submission cuts; it may expose bounded duplicate physical Sling Job records, but only one fence can commit `ExecutionStarted` and only one command-effect attempt occurs.
- A committed no-return checkpoint or terminal fact prevents another effect attempt; earlier physical enqueue cuts may repeat only within Plan 0005's exact outbox/match bounds.
- Waiters receive monotonic facts or a bounded disconnect and can resume from their last revision.
- Temporary and corrupt artifacts are never presented as complete.
- A changed target cannot adopt nonterminal work, and operation-protocol incompatibility cannot prevent the retained control client from inspecting and stopping the owner.
- The release `slingshot` binary contains only the inactive observer composition; the injectable observer and control channel are reachable only from the existing development binary's internal repository-test subcommand, and Cargo metadata still exposes exactly the two binaries fixed by Plan 0001.

- **Done when:** `cargo test -p slingshot-development --test daemon_process_chaos` kills the dedicated real-process production-application composition at every daemon-owned checkpoint/phase and passes the committed seed depth with authenticated runtime/transport contracts, one logical fake-author operation, a manifest-bounded physical-record set, at most one fenced command-effect attempt, coherent durable revisions, honest replay counts, verified artifacts, production observer isolation, and reproducible failure evidence, and `scripts/quality` succeeds.
