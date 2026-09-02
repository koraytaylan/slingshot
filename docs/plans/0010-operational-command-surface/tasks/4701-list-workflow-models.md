---
id: list-workflow-models
title: "List Workflow Models"
workstream: "0047"
kind: task
depends_on:
  - process-identity
  - operational-listing
  - resolve-and-map-resource-path
gated: false
touches:
  - crates/slingshot-domain/src/command/list_workflow_models.rs
  - crates/slingshot-domain/src/command/mod.rs
  - crates/slingshot-domain/tests/fixtures/command-module-inventory.txt
  - crates/slingshot-domain/tests/list_workflow_models.rs
  - "crates/slingshot-domain/tests/fixtures/commands/list_workflow_models/**"
status: done
merged_as: "40945984945206f6a25bb5ea2b38026904a8de78"
---
# List Workflow Models

Starting a workflow requires a model identifier, and an operator has no way to learn one. This task represents the model inventory as a windowed listing, which is the command every other workflow command is reached through.

**Steps:**

1. Commit canonical accepted and refused argument fixtures and exact no-effect failure documents before the implementation, one line per vector, each carrying the note that says what it proves.
2. Implement `ListWorkflowModelsCommand` with an optional `title_prefix` bounded by `MAXIMUM_WORKFLOW_TITLE_BYTES` and an optional `result_window`.
3. Implement the match as the model identifier, its title, and its version when the author reports one.
4. Order matches strictly ascending by model identifier under the shared text order rule, refusing a repeat.
5. Allow the shared discovery failures plus `workflow_inventory_failed`.
6. Supply request-context validation that refuses a match whose title does not carry the requested prefix.

**Tests:**

- An empty listing, a one-row listing, and a strictly ascending listing round-trip byte-identically.
- A repeated or descending model identifier is refused.
- A match whose title does not carry the requested prefix is refused.
- An absent version is omitted rather than serialized as null.
- Each failure document carries exactly its discriminator and proves no effect.

- **Done when:** `cargo test -p slingshot-domain --test list_workflow_models` proves the ordering rule, the prefix rule, the omitted-rather-than-null member, and every closed failure.
