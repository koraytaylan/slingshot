---
id: model-context-protocol-module-scaffold
title: "Model Context Protocol Module Scaffold"
workstream: "0029"
kind: chore
depends_on: []
gated: false
touches:
  - crates/slingshot-command-line/src/model_context_protocol/mod.rs
  - crates/slingshot-command-line/src/model_context_protocol/active_request_registry.rs
  - crates/slingshot-command-line/src/model_context_protocol/application.rs
  - crates/slingshot-command-line/src/model_context_protocol/current_stateless_revision.rs
  - crates/slingshot-command-line/src/model_context_protocol/legacy_initialized_revision.rs
  - crates/slingshot-command-line/src/model_context_protocol/operation_execution.rs
  - crates/slingshot-command-line/src/model_context_protocol/progress_and_cancellation.rs
  - crates/slingshot-command-line/src/model_context_protocol/protocol_diagnostics.rs
  - crates/slingshot-command-line/src/model_context_protocol/resource_catalog.rs
  - crates/slingshot-command-line/src/model_context_protocol/result_projection.rs
  - crates/slingshot-command-line/src/model_context_protocol/schema_projection.rs
  - crates/slingshot-command-line/src/model_context_protocol/size_budget.rs
  - crates/slingshot-command-line/src/model_context_protocol/standard_stream_transport.rs
  - crates/slingshot-command-line/src/model_context_protocol/tool_catalog.rs
  - crates/slingshot-command-line/tests/model_context_protocol_module_scaffold.rs
  - "crates/slingshot-command-line/tests/fixtures/model-context-protocol/module-scaffold/**"
status: done
merged_as: "9beb9be764d21fde0d5a721513ade11de74e34b9"
---
# Model Context Protocol Module Scaffold

Register the complete Plan 0007 protocol-module inventory once so every transport, revision, projection, resource, and execution task begins from a compiling crate-reachable leaf.

**Steps:**

1. Commit an independently ordered fixture containing exactly the thirteen Plan 0007 feature leaves and their single `model_context_protocol` parent before changing the structural root.
2. Adopt Plan 0001's existing `model_context_protocol/mod.rs` root and declare the leaves in this exact order: `active_request_registry`, `application`, `current_stateless_revision`, `legacy_initialized_revision`, `operation_execution`, `progress_and_cancellation`, `protocol_diagnostics`, `resource_catalog`, `result_projection`, `schema_projection`, `size_budget`, `standard_stream_transport`, and `tool_catalog`.
3. Create every declared leaf as a compiling documentation-only structural module whose documentation states only present architectural ownership and contains no placeholder function, type, behavior, planning marker, protocol claim, or feature-specific constant.
4. Add a bidirectional structural test comparing the independent fixture, parent declarations, source files, and this task's exact source footprint. Reject a missing, additional, duplicate, reordered, undeclared, renamed, or multiply parented leaf.
5. Run workspace compilation, documentation warnings, source policy, and semantic documentation review without implementing transport behavior, selecting a protocol revision, changing the top-level command application, or introducing a dependency.

**Tests:**

- The compiled parent exposes exactly the thirteen fixture-listed leaves in the required order, with no wildcard or conditional declaration.
- Every feature source file is reachable through exactly one parent before its dependency-ordered owning task implements it.
- Parent and leaf modules contain only accurate present-state structure, with no behavior, protocol constant, mock, placeholder body, planning language, or undocumented export.
- A source/declaration/fixture/footprint mismatch fails independently of all later Model Context Protocol tests.

- **Done when:** `cargo test -p slingshot-command-line --test model_context_protocol_module_scaffold && RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps` proves the exact thirteen-leaf Plan 0007 inventory is declared once, compiling, present-fact documented, and ready for its dependency-ordered owning tasks.
