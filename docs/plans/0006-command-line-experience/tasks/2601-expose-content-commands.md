---
id: expose-content-commands
title: "Expose Content Commands"
workstream: "0026"
kind: task
depends_on:
  - define-command-invocations
gated: false
touches:
  - crates/slingshot-command-line/src/commands/content.rs
  - crates/slingshot-command-line/tests/content_commands.rs
status: done
merged_as: "ebc7636025dce5be2e354d616be069865d71878f"
---
# Expose Content Commands

Map content loading to the registry's JSON representation request with shared repository-path validation and no direct Adobe Experience Manager access.

**Steps:**

1. Commit invocation-to-request fixtures for valid paths, malformed paths, representation values, optional depth boundaries, supplied/missing caller operation key, and registry rejection.
2. Implement content command validation, registry lookup, typed request construction, generated help metadata, and the registry-derived caller-key requirement for read-classified but NotIntrinsicallyIdempotent `load_content_as_json`.
3. Assert the local daemon client receives the exact request and the network recorder receives none.

**Tests:**

- `content_commands` pins every request field, default and explicit depth, operation-key mapping, and validation diagnostic; a missing key fails before daemon or network access.
- A registry-coverage case proves the command names one existing read-classified, non-intrinsically-idempotent operation rather than inferring idempotency from the read label.

- **Done when:** `cargo test -p slingshot-command-line --test content_commands` proves content loading requires its caller key before access and produces exactly the registered JSON-representation request.
