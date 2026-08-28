---
id: define-command-invocations
title: "Define Command Invocations"
workstream: "0025"
kind: task
depends_on:
  - command-line-module-scaffold
gated: false
touches:
  - crates/slingshot-command-line/src/invocation.rs
  - crates/slingshot-command-line/tests/invocation_parsing.rs
  - crates/slingshot-test-support/fixtures/command-invocations/**
status: planned
merged_as: ""
---
# Define Command Invocations

Define the side-effect-free command tree, leaf-scoped selection/rendering/operation options, complete local command surface, and exact help/version behavior.

**Steps:**

1. Commit accepted and rejected argument-vector fixtures for every leaf-scoped selection/output/detachment/operation-key option, supplied/omitted caller operation key, operation leaf, configuration-check leaf, daemon start/status/ping/stop leaf, bounded operation-list leaf, required expected revision plus exact expected recovery category on operation resume, optional historical author-target digest on list/status/result/artifact and maintenance preview/apply/result, forbidden digest on execution/wait/resume, bounded maintenance-preview criteria, required maintenance-apply preview digest with no fresh criteria, operation-free maintenance-result identifier plus required expected digest and destination, conflict, missing value, and help/version position.
2. Implement the closed invocation types and parser without reading configuration, opening files, starting processes, or connecting to a daemon; model output as ordinary-renderer-only and detachment/operation key as submission-only so a later standard-stream server leaf cannot inherit them as globals.
3. Add parser snapshots and fakes that fail if a parse-only path invokes any external boundary.

**Tests:**

- `invocation_parsing` covers the complete fixture table, leaf-scoped output/detachment/operation-key placement and bounds, and exact structured diagnostics; registry-aware cases require a key for every non-intrinsically-idempotent descriptor before external access.
- Help and version are the only metadata-only cases and assert zero calls to configuration, filesystem, daemon, and network fakes.

- **Done when:** `cargo test -p slingshot-command-line --test invocation_parsing` proves every documented leaf parses deterministically without side effects and only help/version are classified metadata-only.
