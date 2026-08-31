---
id: find-workflow-instances
title: "Find Workflow Instances"
workstream: "0047"
kind: task
depends_on:
  - start-workflow
gated: false
touches:
  - crates/slingshot-domain/src/command/find_workflow_instances.rs
  - crates/slingshot-domain/src/command/property_value.rs
  - crates/slingshot-domain/src/command/mod.rs
  - crates/slingshot-domain/tests/fixtures/command-module-inventory.txt
  - crates/slingshot-domain/tests/find_workflow_instances.rs
  - "crates/slingshot-domain/tests/fixtures/commands/find_workflow_instances/**"
status: done
merged_as: ""
---
# Find Workflow Instances

Active instances and archived ones are the same question asked about different states, so they are one command with a state set rather than two commands that answer differently. Completed and aborted instances are the archived ones.

**Steps:**

1. Commit canonical accepted and refused argument fixtures and exact no-effect failure documents before the implementation, one line per vector, each carrying the note that says what it proves.
2. Implement `FindWorkflowInstancesCommand` with an optional `model_identifier`, an optional `payload_prefix` as a validated repository path, a required non-empty ascending `states` set over the closed instance state set, and an optional `result_window`.
3. Implement the match as the instance identifier, the model identifier, the payload path, the state, and the start time when the author reports one, as a JCR date-time value under the existing property model.
4. Order matches strictly ascending by instance identifier, refusing a repeat.
5. Allow the shared discovery failures plus `workflow_inventory_failed`.
6. Supply request-context validation that refuses a match whose state is outside the requested set, whose model is not the requested one, or whose payload is outside the requested prefix.

**Tests:**

- An empty listing, a one-row listing, and a strictly ascending listing round-trip byte-identically.
- An empty state set is refused, a repeated state is refused, and a descending set is refused.
- A match in a state outside the requested set is refused, and every closed state appears across the fixtures including both archived states.
- A match whose payload is outside the requested prefix is refused, proved on the boundary where the payload equals the prefix.
- An absent start time is omitted rather than serialized as null.

- **Done when:** `cargo test -p slingshot-domain --test find_workflow_instances` proves the non-empty ascending state set, every closed state including the archived ones, all three request-context rules, and every closed failure.
