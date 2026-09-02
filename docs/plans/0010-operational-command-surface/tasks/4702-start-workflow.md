---
id: start-workflow
title: "Start a Workflow"
workstream: "0047"
kind: task
depends_on:
  - list-workflow-models
gated: false
touches:
  - crates/slingshot-domain/src/command/start_workflow.rs
  - crates/slingshot-domain/src/command/mod.rs
  - crates/slingshot-domain/tests/fixtures/command-module-inventory.txt
  - crates/slingshot-domain/tests/start_workflow.rs
  - "crates/slingshot-domain/tests/fixtures/commands/start_workflow/**"
status: done
merged_as: "40945984945206f6a25bb5ea2b38026904a8de78"
---
# Start a Workflow

A workflow is how content actually moves through review and publication, and nothing in the registry can start one. This task represents starting one model against one payload with the metadata a model may need.

**Steps:**

1. Commit canonical accepted and refused argument fixtures and exact no-effect failure documents before the implementation, one line per vector, each carrying the note that says what it proves.
2. Implement `StartWorkflowCommand` with a `model_identifier`, a `payload_path`, an optional `title`, an optional `comment` bounded by `MAXIMUM_WORKFLOW_COMMENT_BYTES`, and an optional `metadata` map of at most `MAXIMUM_WORKFLOW_METADATA_ENTRIES` ascending bounded text entries.
3. Refuse a metadata key that repeats, refuse an unordered map rather than sorting it, and bound every value by `MAXIMUM_PROPERTY_STRING_BYTES`.
4. Implement `StartWorkflowResult` carrying the instance identifier the author minted, the model identifier, and the instance's state.
5. Allow exactly `model_not_found`, `model_invalid`, `payload_not_found`, `payload_access_denied`, `metadata_rejected`, `platform_control_rejected`, and `platform_control_outcome_unknown`.
6. Supply request-context validation that refuses a result naming another model than the one requested.

**Tests:**

- Every accepted vector round-trips byte-identically, with and without each optional member.
- The metadata map is proved at its entry bound and one past it, refuses a repeat, and refuses a descending order.
- The comment and title are each proved at their exact bound and one past it.
- A result naming another model is refused, while any instance identifier is accepted because the author mints it.
- Each failure document carries exactly its discriminator and the value it names and proves no effect.

- **Done when:** `cargo test -p slingshot-domain --test start_workflow` proves the ascending bounded metadata, both sides of every text bound, the model echo rule, and every closed failure.
