---
id: compose-command-line-application
title: "Compose Command-Line Application"
workstream: "0027"
kind: task
depends_on:
  - check-selected-configuration
  - classify-exits-and-interrupts
  - list-and-maintain-operations
gated: false
touches:
  - crates/slingshot-command-line/src/application.rs
  - crates/slingshot-command-line/src/daemon_answer.rs
  - crates/slingshot-command-line/src/daemon_request.rs
  - crates/slingshot-command-line/src/command_line.rs
  - crates/slingshot-command-line/src/main.rs
  - crates/slingshot-command-line/tests/application_dispatch.rs
  - "crates/slingshot-command-line/tests/fixtures/application-dispatch/**"
status: done
merged_as: "c2ac8d910a2efdc6f5d8d1aa0600b981e14325cf"
---
# Compose Command-Line Application

Compose every parsed command leaf with its target, service, renderer, and exit classification through one exhaustive application boundary.

**Steps:**

1. Commit a dispatch matrix covering help, version, configuration check, daemon lifecycle, every registry command, operation list/status/wait/resume/result/artifact, separate inline/operation-free-associated maintenance preview/apply plus maintenance-result metadata/read retrieval, and every output mode. Include one-at-a-time daemon-runtime, author-agent-transport, canonical-JSON-contract, limits, and role-schema provenance mismatch.
2. Implement `CommandLineApplication` with injected typed daemon-runtime and author-agent-transport contracts plus configuration, process, local-protocol, filesystem, clock, signal, output, and network boundaries and an exhaustive match over `CommandInvocation`.
3. Verify the module scaffold still declares exactly the ordered leaf inventory `content`, `package`, `replication`, `configuration`, `page_query`, `path_query`, `asset_query`, and `page_mutation`, then wire those already crate-reachable leaves plus the observation/maintenance services through the application and production binary entries so each invocation reaches exactly one service, produces exactly one final rendering decision, and returns one exit classification. Do not reopen a shared parent module or conditionally declare a leaf.
4. Add negative fakes proving help/version invoke no external boundary, configuration check invokes only local configuration/filesystem boundaries, and daemon-backed leaves cannot bypass target/revision validation.

**Tests:**

- `application_dispatch` compares the invocation catalog, exact eight-leaf `commands/mod.rs` inventory, and dispatch matrix as equal ordered sets and fails for a missing/duplicate/reordered/orphan leaf.
- Call-count assertions prove every parser path reaches exactly one expected service and no leaf falls through, double-renders, performs an unowned side effect, or reaches a versioned service after runtime/transport provenance refusal.
- Binary-entry cases prove returned classifications become the documented process exit values.

- **Done when:** `cargo test -p slingshot-command-line --test application_dispatch` proves the exact eight command leaves are declared once and every parser leaf reaches exactly one provenance-checked owned service/rendering path, including inline/operation-free-associated maintenance and maintenance-result metadata/read retrieval, and the production entry maps its result to the documented exit taxonomy.
