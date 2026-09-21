---
id: protocol-tool-runner-leaf
title: "Protocol Tool Runner Leaf"
workstream: "0031"
kind: chore
depends_on:
  - model-context-protocol-application-entry
gated: false
touches:
  - crates/slingshot-command-line/src/protocol_tool_runner.rs
  - crates/slingshot-command-line/src/command_line.rs
  - crates/slingshot-command-line/src/lib.rs
  - crates/slingshot-command-line/src/application.rs
  - crates/slingshot-command-line/src/artifact_download.rs
  - crates/slingshot-command-line/src/model_context_protocol/application.rs
  - crates/slingshot-command-line/src/model_context_protocol/operation_execution.rs
  - crates/slingshot-command-line/src/model_context_protocol/resource_catalog.rs
  - crates/slingshot-command-line/src/model_context_protocol/size_budget.rs
  - crates/slingshot-command-line/tests/model_context_protocol_application.rs
  - crates/slingshot-command-line/tests/fixtures/command-line-module-scaffold/leaves.txt
  - crates/slingshot-development/tests/fixtures/workspace-module-map/module-ownership.txt
status: done
merged_as: ""
---
# Protocol Tool Runner Leaf

Serve a protocol call and a resource read through the one application a command line uses, in a leaf of its own.

**Steps:**

1. Move the tool-call translation and the product boundaries behind it out of `command_line.rs` into `protocol_tool_runner.rs`, re-exporting `tool_invocation` where callers have always reached it, so the command line stays under the file ceiling and the translation has one home.
2. Extend the runner boundary with an artifact fetch: a resource address names its own profile, environment, and target, and a fetch selects the daemon from the address rather than from the process's own selection, because assuming the latter would answer an address for one target with another target's bytes.
3. Fetch the bytes through the same `CommandLineApplication` a command line uses, verifying them against the length and digest the daemon declared before the transfer began, and refuse at the declared length an artifact past what the asking reader can carry.
4. Answer an artifact resource read with the bytes themselves: JSON and textual artifacts as `text`, every other media type as base64 in `blob`, and no partial contents ever.

**Tests:**

- `model_context_protocol_application` proves an artifact address is answered with the exact bytes the daemon vouched for, that a binary artifact travels base64 rather than lossy text, and that a fetch which cannot complete is a local failure and never an empty document.

- **Done when:** `cargo test -p slingshot-command-line --test model_context_protocol_application` proves a resource address of any published shape is answered through the one shared application, with artifact bytes verified whole before they are returned and no shape answered by invented bytes.
