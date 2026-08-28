---
id: walking-skeleton-process-proof
title: "Walking Skeleton Process Proof"
workstream: "0003"
kind: task
depends_on:
  - concurrent-explicit-start
gated: false
touches:
  - crates/slingshot-test-support/src/lib.rs
  - crates/slingshot-test-support/src/process_harness.rs
  - crates/slingshot-test-support/src/runtime_harness.rs
  - crates/slingshot-test-support/src/supervised_child.rs
  - crates/slingshot-command-line/tests/walking_skeleton.rs
  - "crates/slingshot-command-line/tests/fixtures/walking-skeleton/**"
status: planned
merged_as: ""
---
# Walking Skeleton Process Proof

The walking claim is established with independent operating-system processes, not an in-memory server that bypasses startup, locks, framing, or output streams.

**Steps:**

1. Implement reusable path-only executable, process, and temporary-runtime harnesses in `slingshot-test-support` with a barrier, real-monotonic deadline polling, owned-child accounting/reaping, output capture, and one private supervision channel per detached test daemon. The test spawn adapter is invoked only by the elected starter but leaves a supervisor owning the exact unreaped `std::process::Child`/native stable process handle until one atomic exit-or-terminate-and-wait disposition. The numeric process identifier is recorded only for output correlation; the harness never looks it up, identity-checks then signals it, or sends a PID-based termination. Supervision tokens are unguessable, instance-bound, and cannot target a replacement. The harness imports no command-line, daemon, configuration, agent-connection, or development type.
2. Hand-author normalized start, ping, not-running, slow-client-release, and failure outputs, replacing nondeterministic process/request identifiers only through an explicit fixture normalization function.
3. First run existing-only ping against absence and prove no child/runtime mutation; then launch at least twenty `slingshot daemon start` clients simultaneously for one target and assert every readiness response identifies the same single daemon. In a separate instrumented cohort, terminate the elected client before spawn and after its one spawn but before readiness observation; assert operating-system election-lock release, connect-first successor takeover, and exactly one resulting live owner in both branches.
4. Fill the manifest connection capacity with a cohort split across no initial frame, partial length prefix, partial payload, byte-drip, and nonreading response peers. Observe the product daemon's real deadline behavior with a monotonic upper bound equal to the manifest server deadline plus its exact scheduling tolerance; use no exact lower-bound timing assertion and no fixed sleep. Prove all incomplete/blocked peers release and a later valid ping remains serviceable. For responsive cleanup, send `daemon.stop` with the nonce returned by that daemon's ping and wait through the manifest cooperative-stop deadline. If the daemon is deliberately unresponsive, send one terminate request over its retained supervision channel; the supervisor terminates and waits through its exact stable child handle under the manifest deadline without any PID lookup. Wait for owner-lock release, prove ping reports absence without spawning, release another start cohort to recover one fresh nonce, prove the old nonce and supervision token cannot affect it, and start a second target to prove both owners remain independently reachable.

**Tests:**

- Each barrier-start cohort produces exactly one daemon process and one successful readiness response per client.
- Elected-client crash before spawn transfers election to one successor; crash after spawn makes successors join that responsive child or create exactly one absence-proved replacement, never a second concurrent owner.
- Every start response has the same daemon process identifier and its own request correlation; following pings report that existing owner.
- Ping before first start and after forced termination reports not running, attempts no spawn/start lock, and leaves state unchanged; the second start cohort recovers one new nonce.
- Two target namespaces run concurrently and never answer with each other's target values.
- Captured standard output contains result frames only; daemon and startup diagnostics never contaminate it.
- The slow-client cohort cannot strand connection capacity: every peer closes by the declared real deadline plus the named scheduling tolerance and a later valid process ping succeeds before its own harness deadline, with no fixed sleep or cross-process injected-clock claim.
- Responsive cleanup uses only current-nonce `daemon.stop`; a stale nonce cannot stop the replacement. Unresponsive cleanup uses only the instance-bound supervisor's still-owned unreaped child handle, and reused numeric process identifiers plus stale supervision-token fixtures cannot redirect termination.
- On success and induced assertion failure, the harness reaps every client child it owns, cooperatively stops or stable-handle-terminates and waits for every supervised detached daemon, observes owner-lock release, and only then removes its temporary runtime root. The one matching current-native run also records endpoint permissions/access control lists and detached-daemon containment as untrusted local evidence.

- **Done when:** `cargo test -p slingshot-command-line --test walking_skeleton -- --nocapture` uses the manifest's exact 20-client/deadline/capacity values to prove current-native explicit starts converge on one daemon, slow clients release, existing-only ping never spawns, start recovers one fresh-nonce successor, and current-nonce cooperative stop or retained stable-child supervision—not PID check-then-signal—leaves no owned process, held lock, or temporary state.
