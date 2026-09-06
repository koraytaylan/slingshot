---
id: compiled-processes-prove-the-product
title: "Compiled Processes Prove The Product"
workstream: "0076"
kind: task
depends_on: ["the-product-has-one-author-port", "readiness-follows-complete-durable-startup", "the-daemon-serves-versioned-operations", "schedulers-claim-before-execution", "model-context-protocol-projects-the-real-surface", "product-failures-reach-redacted-diagnostics"]
gated: false
touches:
  - crates/slingshot-development/tests/product_runtime_process.rs
  - crates/slingshot-development/tests/operation_submission_process.rs
  - crates/slingshot-development/tests/fixtures/product-runtime/**
  - README.md
  - ARCHITECTURE.md
  - docs/DAEMON.md
  - docs/COMMANDS.md
  - docs/MODEL_CONTEXT_PROTOCOL.md
status: completed
merged_as: "c579ad2"
---
# Compiled Processes Prove The Product

Current successful "product" tests call storage and daemon modules directly. They do not start the binary, perform hello over the endpoint, or prove an operation can reach an author adapter. Product documents consequently make mutually incompatible claims about whether that runtime exists.

**Steps:**

1. Launch the compiled `slingshot` binary in isolated real runtime/state/config roots with a protocol-faithful fake author; interact only through public CLI, local socket, and standard-stream interfaces.
2. Compare the command registry exactly across CLI and both Model Context Protocol revisions, and drive every descriptor through canonical schema refusal plus a generated fake-author terminal outcome; cover inline, artifact, recovery, cancellation, maintenance, and unavailable transport dispositions.
3. Crash and restart after startup, durable admission, scheduler claim, remote send, receipt, artifact publication, result settlement, and maintenance database apply; prove the hardened old/new invariants and one logical effect. Overlap schedulers and clients under the same frozen clock.
4. Remove or rename direct-module tests that claim product composition, retaining them as unit/integration tests with accurate scope.
5. Rewrite product documentation from the process-proved behavior, resolving the executor/startup/no-AEM contradictions and stating explicit unavailable/live-author boundaries.

- **Done when:** the compiled processes alone prove discovery, execution, persistence, recovery, artifacts, maintenance, diagnostics, and shutdown across every catalog command class, and every current-state product document matches that tested composition without a direct-module substitute.

## Implementation checkpoint

The compiled-process composition suite now accounts for the explicit runtime
test-host executable alongside the shipped binaries, so all-features product
verification no longer reports a false missing-binary failure. The process
tests cover executor outcome taxonomy, recovery evidence, progress delivery,
invocation idempotency, and product/test-support dependency separation; the
daemon walking skeleton and local-server suites cover startup, endpoint, and
shutdown boundaries.
