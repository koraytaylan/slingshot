---
id: terminate-workflow-instance
title: "Terminate a Workflow Instance"
workstream: "0047"
kind: task
depends_on:
  - inspect-workflow-instance
gated: false
touches:
  - crates/slingshot-domain/src/command/terminate_workflow_instance.rs
  - crates/slingshot-domain/src/command/mod.rs
  - crates/slingshot-domain/tests/fixtures/command-module-inventory.txt
  - crates/slingshot-domain/tests/terminate_workflow_instance.rs
  - "crates/slingshot-domain/tests/fixtures/commands/terminate_workflow_instance/**"
status: planned
merged_as: ""
---
# Terminate a Workflow Instance

An instance stuck on a step nobody will complete has to be endable, and ending it is destructive in the exact sense this plan means: something already in effect stops being in effect.

**Steps:**

1. Commit canonical accepted and refused argument fixtures and exact no-effect failure documents before the implementation, one line per vector, each carrying the note that says what it proves.
2. Implement `TerminateWorkflowInstanceCommand` with an `instance_identifier` and nothing else.
3. Implement the result carrying the instance identifier and the state observed after the termination, so an instance that refused to end is visible rather than reported as success.
4. Allow exactly `instance_not_found`, `instance_access_denied`, `instance_not_terminable`, `platform_control_rejected`, and `platform_control_outcome_unknown`.
5. Supply request-context validation that refuses a result naming another instance.

**Tests:**

- Every accepted vector round-trips byte-identically and refuses an unknown member.
- The observed state is a closed instance state and an unknown spelling is refused.
- An already-terminated instance produces `instance_not_terminable` rather than a success, proved by a fixture.
- Each failure document carries exactly its discriminator and `instance_identifier` and proves no effect.
- A result naming another instance is refused.

- **Done when:** `cargo test -p slingshot-domain --test terminate_workflow_instance` proves the observed-state answer, the already-ended refusal, every closed failure, and request-context validation.
