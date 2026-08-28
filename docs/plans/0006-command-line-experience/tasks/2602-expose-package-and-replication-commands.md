---
id: expose-package-and-replication-commands
title: "Expose Package And Replication Commands"
workstream: "0026"
kind: task
depends_on:
  - define-command-invocations
gated: false
touches:
  - crates/slingshot-command-line/src/commands/package.rs
  - crates/slingshot-command-line/src/commands/replication.rs
  - crates/slingshot-command-line/tests/package_and_replication_commands.rs
status: planned
merged_as: ""
---
# Expose Package And Replication Commands

Map recursive replication and filtered package download into their distinct mutation and artifact-producing registry requests.

**Steps:**

1. Commit request fixtures for shallow/recursive replication and bounded package names plus ordered root/include/exclude package patterns, including malformed/repeated options, missing/supplied caller operation keys, and the exact `download_content_package` `1.0.0` descriptor identity, limits digest, FileVault profile/filter failures, and eight evaluation-budget literals.
2. Implement both command mappings while preserving pattern order and leaving pattern interpretation to the agent operation.
3. Pin mutation classification, artifact expectation, help text, exact local request values, and the complete registry-owned FileVault failure inventory; define no CLI alias for profile unsupported, filter unrepresentable, or any package failure/budget value.

**Tests:**

- `package_and_replication_commands` covers recursion and every ordered filter combination.
- Registry checks prove both operations consume exact `1.0.0` plus the canonical limits/schema digests, are non-intrinsically-idempotent, and require a caller key before daemon access; package download also declares its exact artifact contract and revised closed failure set.

- **Done when:** `cargo test -p slingshot-command-line --test package_and_replication_commands` proves replication and package download require caller keys before daemon access and both argument sets map losslessly to their registry operations.
