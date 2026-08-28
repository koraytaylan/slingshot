---
id: command-line-module-scaffold
title: "Command-Line Module Scaffold"
workstream: "0025"
kind: chore
depends_on: []
gated: false
touches:
  - crates/slingshot-command-line/src/lib.rs
  - crates/slingshot-command-line/src/application.rs
  - crates/slingshot-command-line/src/invocation.rs
  - crates/slingshot-command-line/src/target_selection.rs
  - crates/slingshot-command-line/src/configuration_check.rs
  - crates/slingshot-command-line/src/commands/mod.rs
  - crates/slingshot-command-line/src/commands/content.rs
  - crates/slingshot-command-line/src/commands/package.rs
  - crates/slingshot-command-line/src/commands/replication.rs
  - crates/slingshot-command-line/src/commands/configuration.rs
  - crates/slingshot-command-line/src/commands/page_query.rs
  - crates/slingshot-command-line/src/commands/path_query.rs
  - crates/slingshot-command-line/src/commands/asset_query.rs
  - crates/slingshot-command-line/src/commands/page_mutation.rs
  - crates/slingshot-command-line/src/predicate_arguments.rs
  - crates/slingshot-command-line/src/property_document.rs
  - crates/slingshot-command-line/src/operation_submission.rs
  - crates/slingshot-command-line/src/artifact_download.rs
  - crates/slingshot-command-line/src/artifact_staging_lock.rs
  - crates/slingshot-command-line/src/artifact_staging_metadata.rs
  - crates/slingshot-command-line/src/operation_observation.rs
  - crates/slingshot-command-line/src/artifact_access.rs
  - crates/slingshot-command-line/src/human_renderer.rs
  - crates/slingshot-command-line/src/machine_outcome_envelope.rs
  - crates/slingshot-command-line/src/machine_readable_renderer.rs
  - crates/slingshot-command-line/src/progress_renderer.rs
  - crates/slingshot-command-line/src/exit_classification.rs
  - crates/slingshot-command-line/src/interrupt.rs
  - crates/slingshot-command-line/src/operation_maintenance.rs
  - crates/slingshot-command-line/tests/command_line_module_scaffold.rs
  - "crates/slingshot-command-line/tests/fixtures/command-line-module-scaffold/**"
status: planned
merged_as: ""
---
# Command-Line Module Scaffold

Register the complete Plan 0006 source-module inventory once so every command-line feature task begins from a compiling crate-reachable leaf and never edits a shared parent merely to expose its implementation.

**Steps:**

1. Commit an independently ordered fixture mapping every Plan 0006 source leaf to the `slingshot-command-line` crate and its crate-root or command-family parent before changing a module declaration.
2. Adopt Plan 0001's existing `lib.rs` and `commands/mod.rs` structural roots. Declare exactly the top-level and eight command-family leaves listed in this task's footprint, with the command leaves ordered as `content`, `package`, `replication`, `configuration`, `page_query`, `path_query`, `asset_query`, and `page_mutation`.
3. Create each declared leaf as a compiling documentation-only structural module. Its module documentation states only its present architectural ownership and contains no placeholder function, type, behavior, planning marker, or feature claim.
4. Add a bidirectional structural test comparing the independent fixture, parent declarations, source files, and this task's exact source footprint. Reject a missing, additional, duplicate, reordered, undeclared, or misowned leaf.
5. Run workspace compilation, documentation warnings, source policy, and semantic documentation review over the complete scaffold without changing a dependency, public behavior, wire contract, limit, or feature-owned integration test.

**Tests:**

- Every declared Plan 0006 source leaf exists, is reachable from exactly one owning parent, and matches the independent ownership fixture.
- The exact eight command-family declarations have the required order, while every other top-level leaf appears exactly once under the crate root.
- Parent modules contain no Plan 0006 behavioral implementation, and feature leaves contain only accurate present-state module documentation until their dependency-ordered owning tasks implement them.
- An undeclared source file, declaration without a file, duplicate parent, changed order, wildcard declaration, placeholder body, planning language, undocumented exported item, external dependency use, or feature-specific constant fails the scaffold checks.

- **Done when:** `cargo test -p slingshot-command-line --test command_line_module_scaffold && RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps` proves the exact Plan 0006 module inventory is declared once, compiling, present-fact documented, crate-reachable, and ready for its dependency-ordered owning tasks.
