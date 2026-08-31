---
id: inspect-workflow-instance
title: "Inspect a Workflow Instance"
workstream: "0047"
kind: task
depends_on:
  - find-workflow-instances
gated: false
touches:
  - crates/slingshot-domain/src/command/inspect_workflow_instance.rs
  - crates/slingshot-domain/src/command/mod.rs
  - crates/slingshot-domain/tests/fixtures/command-module-inventory.txt
  - crates/slingshot-domain/tests/inspect_workflow_instance.rs
  - "crates/slingshot-domain/tests/fixtures/commands/inspect_workflow_instance/**"
status: done
merged_as: "e71740fdc51e0a18c337831b46347358209836f2"
---
# Inspect a Workflow Instance

A stalled workflow is stalled at a work item, and the listing cannot say which. This task represents one instance in full, including its open work items and who they are assigned to.

**Steps:**

1. Commit canonical accepted and refused argument fixtures and exact no-effect failure documents before the implementation, one line per vector, each carrying the note that says what it proves.
2. Implement `InspectWorkflowInstanceCommand` with an `instance_identifier` and nothing else.
3. Implement the result carrying the instance identifier, the model identifier, the payload path, the state, and the work items in ascending identifier order, at most `MAXIMUM_WORKFLOW_WORK_ITEMS`, bounded overall by `MAXIMUM_OPERATIONAL_INSPECTION_RESULT_BYTES`.
4. Implement the work item as its identifier, its node title, and its assignee as an authorizable identifier when there is one; an assignee is an identifier and never a display name, so nothing here carries a person's name.
5. Allow exactly `instance_not_found`, `instance_access_denied`, `workflow_inventory_failed`, and `result_budget_exceeded`.
6. Supply request-context validation that refuses a result naming another instance.

**Tests:**

- Every accepted vector round-trips byte-identically, with an empty work-item list and with a full one.
- The work-item list is proved at `MAXIMUM_WORKFLOW_WORK_ITEMS` and one past it, and a repeated or descending item identifier is refused.
- The result budget is proved at its exact bound and one past it.
- An absent assignee is omitted rather than serialized as null.
- A result naming another instance is refused.

- **Done when:** `cargo test -p slingshot-domain --test inspect_workflow_instance` proves the ascending bounded work items, both sides of both bounds, the omitted-rather-than-null assignee, and every closed failure.
