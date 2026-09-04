---
id: model-context-protocol-projects-the-real-surface
title: "Model Context Protocol Projects The Real Surface"
workstream: "0075"
kind: task
depends_on: ["schedulers-claim-before-execution"]
gated: false
touches:
  - crates/slingshot-command-line/src/command_line.rs
  - crates/slingshot-command-line/src/model_context_protocol/application.rs
  - crates/slingshot-command-line/src/model_context_protocol/operation_execution.rs
  - crates/slingshot-command-line/src/model_context_protocol/tool_catalog.rs
  - crates/slingshot-command-line/src/model_context_protocol/resource_catalog.rs
  - crates/slingshot-command-line/tests/model_context_protocol_application.rs
  - crates/slingshot-test-support/fixtures/model-context-protocol/**
status: planned
merged_as: ""
---
# Model Context Protocol Projects The Real Surface

The shipped protocol server answers tool and resource methods with empty arrays and content. Existing catalogs and execution/resource adapters are never supplied to `ServerApplication`, so golden tests certify a stub rather than the claimed surface.

**Steps:**

1. Construct the server with resolved target/profile daemon access, exact tool and resource catalogs, operation identity/execution, progress/cancellation, and result projection instead of a method-name-to-empty-payload function.
2. Project the complete command registry and schemas into both supported protocol eras; keep revision-specific envelope decoration separate from the shared capability set.
3. Dispatch tool calls through canonical argument validation and the real daemon service, and resource reads through target-qualified operation, artifact, and operation-free maintenance identities with existing lifecycle/access checks.
4. Replace empty golden listings with exact registry equality and run process-level list, valid call, schema refusal, daemon/domain refusal, progress/cancel, result/artifact read, unknown resource, reconnect, and legacy-session cases.

- **Done when:** both protocol revisions discover exactly the installed command/resource surface and every call/read reaches the same daemon result or typed refusal as the CLI, with no empty stub or alternate execution path.
