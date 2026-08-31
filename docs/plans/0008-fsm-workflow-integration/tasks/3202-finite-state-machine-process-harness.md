---
id: finite-state-machine-process-harness
title: "FSM Process Harness"
workstream: "0032"
kind: task
depends_on:
  - pinned-finite-state-machine-executable
gated: false
touches:
  - crates/slingshot-development/src/finite_state_machine_process_harness.rs
  - crates/slingshot-development/src/lib.rs
  - crates/slingshot-development/src/main.rs
  - crates/slingshot-development/tests/fixtures/finite-state-machine-process-harness/**
  - crates/slingshot-development/tests/finite_state_machine_process_harness.rs
status: done
merged_as: "aa2b7bce2f8a9bc66a36f7b3a41a505c0e0c0982"
---
# FSM Process Harness

Provide one outermost development harness that runs the real FSM, Slingshot protocol child, target daemon, and FakeAuthor as observable operating-system processes.

**Steps:**

1. Commit process-readiness, clean-shutdown, early-exit, timeout, log-capture, path-with-spaces, child-reaping, private-configuration-root, hostile-temporary-production-root sentinel, exact upstream contract identity, and independent repository/sidecar/embedded/`Hello`/capability drift fixtures before implementation.
2. Implement `FiniteStateMachineProcessHarness` and one development-crate-only child entry in `slingshot-development`, consuming test support's explicit `FiniteStateMachineExecutable` path plus isolated configuration, daemon-state, FSM-store, handler, and log directories. The exact child form is `slingshot-development finite-state-machine-process-child --configuration-root <absolute-private-root> --hostile-account-home <absolute-sentinel-root> --role <command-line|model-context-protocol|daemon> -- <production-arguments>`. It constructs Plan 0004's typed explicit test configuration-root source, supplies the sentinel root as the fake account/platform resolver's would-be production result, and enters the unchanged production application or daemon composition. The installed `slingshot` binary has neither this subcommand nor a configuration-root argument/environment selector.
3. Start FakeAuthor and write the selected profile only below the private root. Put a conflicting secret-shaped profile below the second temporary sentinel root without touching the actual user's directories. Use the development child entry for every direct client and daemon; inject its daemon-spawn adapter so auto-started or replacement daemons re-enter the same child entry with the identical two roots and `daemon` role; and expand every FSM handler `argv[0]` to that entry with `model-context-protocol` role. Recompute and compare both exact upstream contract identities across policy bytes/sidecars, embedded typed contracts, the compatibility manifest, target-daemon `Hello`, and FakeAuthor capability; persist that exact `FiniteStateMachineCompatibilityIdentity` in the scenario receipt. Validate and add a fixture machine with the real FSM command line, create an instance, and only after this gate succeeds send a starting event and start `fsm execute`; prove drift leaves no daemon operation, author logical operation, physical Sling record, or effect, and prove every real Slingshot composition uses the private fixture and never opens or reports the sentinel.
4. Observe readiness through protocol probes and explicit output markers rather than fixed sleeps, and query instance state through the real FSM JSON command-line surface.
5. On success, failure, panic, or Drop, terminate and reap every child and report bounded captured standard output and standard error.

**Tests:**

- A minimal no-effect fixture validates, loads, creates an instance, accepts an event, and reports state through the real executable.
- Early daemon, FSM, or Slingshot child exit identifies the process and bounded capture.
- Readiness timeout uses an injected named duration and leaves no child process.
- Paths containing spaces work for every temporary directory and executable argument.
- Repeated harness construction and teardown leaves no process, socket, partial artifact, or shared state.
- Every direct or auto-started Slingshot client, Model Context Protocol server, and original/replacement daemon receives the same typed private test-root source through the exact development child form; wrong/missing role or root arguments fail before production entry, changing ambient `HOME`, XDG, or Windows profile variables and preloading the fake resolver's sentinel root cannot alter selection, state, output, or traces, no actual user directory is touched, and the installed executable exposes no equivalent override.
- Independently stale, malformed, or missing daemon-runtime and author-transport format/digest values in policy bytes, sidecar, embedded contract, compatibility manifest, `Hello`, or capability fail before a starting event or operation/effect; an exact match is recorded as the workflow provenance identity and rechecked against durable terminal records.

- **Done when:** `SLINGSHOT_FINITE_STATE_MACHINE_EXECUTABLE=<finite-state-machine-executable> SLINGSHOT_EXECUTABLE=<slingshot-executable> cargo test -p slingshot-development --test finite_state_machine_process_harness` drives the pinned real FSM through its public command line only after exact upstream contract provenance agrees, proves every drift case leaves no daemon operation, author logical operation, physical Sling record, or effect, routes every real Slingshot role through Plan 0004's typed private test-root source without adding a production override, proves a hostile temporary production-root sentinel is untouched, and reaps every process in success, early-exit, timeout, and repeated-run cases.
