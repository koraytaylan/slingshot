---
id: resolve-command-targets
title: "Resolve Command Targets"
workstream: "0025"
kind: task
depends_on:
  - define-command-invocations
gated: false
touches:
  - crates/slingshot-command-line/Cargo.toml
  - crates/slingshot-command-line/src/target_selection.rs
  - crates/slingshot-command-line/tests/target_selection.rs
status: done
merged_as: "1e029a33d7e47b7c09786d94f87400b1ec58a7dd"
---
# Resolve Command Targets

Resolve command profile and environment options through the typed Plan 0002 selector and produce the canonical daemon namespace.

**Steps:**

1. Write selection tables for explicit names, defaults, conflicting repeats, missing targets, complete configuration-check targets, name-only daemon lifecycle targets, and help/version exemptions.
2. Implement complete selected-environment resolution for configuration check, operations, observation, listing, maintenance, and start; implement name-pair-only resolution for status/ping/stop, with both producing the same canonical namespace and stable mapping of diagnostics to the command exit taxonomy.
3. Derive the current nonsecret `AuthorTargetIdentity` and `SelectedEnvironmentRevision` for every complete target and prove all daemon execution requests carry both values while lifecycle probes use only the canonical namespace pair.
4. Prove help and version return before profile discovery and daemon lifecycle commands never incorporate endpoint or credential data.

**Tests:**

- `target_selection` pins every explicit/default result and error envelope.
- Invalid profile content blocks operation/start but does not block an explicitly named status, ping, or stop against the existing namespace.
- Help and version assert the profile loader is never called; configuration check asserts complete selection occurs.

- **Done when:** `cargo test -p slingshot-command-line --test target_selection` proves every daemon execution leaf carries one stable namespace, current author-target identity, and selected-environment revision while only help/version need no target.
