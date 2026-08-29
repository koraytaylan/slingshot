---
id: concurrent-explicit-start
title: "Concurrent Explicit Start"
workstream: "0003"
kind: task
depends_on:
  - daemon-ping-service
gated: false
touches:
  - crates/slingshot-command-line/src/explicit_daemon_start.rs
  - crates/slingshot-command-line/src/daemon_connection.rs
  - crates/slingshot-command-line/src/command_line.rs
  - crates/slingshot-command-line/src/main.rs
  - crates/slingshot-command-line/tests/explicit_daemon_start.rs
status: done
merged_as: "3b1576d1a83e432277bbfd5533032def993245b9"
---
# Concurrent Explicit Start

Explicit daemon start is a convergence protocol: every start caller either reaches the existing daemon or waits for the elected starter. Ping is a retained existing-owner probe and never invokes that protocol.

**Steps:**

1. Write process-spawner and virtual-deadline cases for explicit start against absent/starting/ready/failed/stale/version-mismatched state, elected-client exit before spawn, elected-client exit after one spawn but before readiness observation, plus ping against absent/stale/ready state.
2. Extend the exhaustive product-binary dispatcher while preserving its version/help behavior, and compose `slingshot --profile <value> --environment <value> daemon start` as connect-first, distinct startup-election-lock acquisition, mandatory connect/readiness recheck after election, absolute self-executable spawn with platform detachment and isolated streams, readiness nonce verification, and bounded retained-ping retries under the exact foundation-contract total/retry-delay limits. Hold election through responsive ping or terminal failure; never transfer it to the child or use it as the daemon owner lock.
3. Ensure only the election-lock holder spawns. Losing start callers wait, retry connection, and then retry election under the same absolute deadline. After winner death releases election, the successor first joins any responsive spawned child; it spawns a replacement only after owner-lock/readiness evidence proves absence, so winner-crash takeover still produces one live owner.
4. Add `slingshot --profile <value> --environment <value> daemon ping` as existing-owner-only. Absence/stale recovery reports not running without acquiring the startup-election lock, spawning, or waiting for readiness.
5. Emit one structured result on standard output and diagnostics only on standard error for both commands.

**Tests:**

- An already responsive daemon causes zero spawn attempts for start and ping.
- Concurrent explicit starts against absence cause one spawn and all receive the same diagnostic process identifier plus the same current readiness nonce.
- If the elected client exits before spawn, one successor spawns; if it exits after its one spawn but before readiness observation, successors join that child when it becomes responsive and otherwise perform exactly one absence-proved takeover spawn. No path transfers the startup-election lock or counts it as daemon ownership.
- Ping against absence/stale state makes zero spawn and startup-election-lock attempts and leaves runtime state unchanged.
- Startup failure, deadline expiry, invalid readiness nonce, and protocol-version mismatch each produce a distinct exit and no false success.
- A stale endpoint is recovered only after ownership proof; a live but slow daemon is not replaced.
- Help and version paths do not inspect or mutate a runtime namespace.

- **Done when:** `cargo test -p slingshot-command-line --test explicit_daemon_start` proves exact-manifest-bounded explicit start-or-join with one current-native detached spawn and one shared current nonce under concurrency, existing-only ping with zero spawn/start-lock effects under absence, and zero runtime effects for help/version.
