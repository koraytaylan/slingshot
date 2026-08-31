---
id: set-workflow-instance-suspension
title: "Set a Workflow Instance Suspension"
workstream: "0047"
kind: task
depends_on:
  - terminate-workflow-instance
gated: false
touches:
  - crates/slingshot-domain/src/command/set_workflow_instance_suspension.rs
  - crates/slingshot-domain/src/command/mod.rs
  - crates/slingshot-domain/tests/set_workflow_instance_suspension.rs
  - "crates/slingshot-domain/tests/fixtures/commands/set_workflow_instance_suspension/**"
status: planned
merged_as: ""
---
# Set a Workflow Instance Suspension

Suspending and resuming are one decision with two values, so they are one command. Two commands would be two places for the state machine to disagree with itself.

**Steps:**

1. Commit canonical accepted and refused argument fixtures and exact no-effect failure documents before the implementation, one line per vector, each carrying the note that says what it proves.
2. Implement `SetWorkflowInstanceSuspensionCommand` with an `instance_identifier` and a closed `requested_state` of `suspended` or `running`.
3. Implement the result carrying the instance identifier and the state observed after the change.
4. Allow exactly `instance_not_found`, `instance_access_denied`, `instance_not_suspendable`, `platform_control_rejected`, and `platform_control_outcome_unknown`.
5. Supply request-context validation that refuses a result naming another instance.

**Tests:**

- Both requested states round-trip byte-identically and an unknown spelling is refused.
- The observed state is a closed instance state, and a result observing a state that is neither suspended nor running is accepted, because a terminated instance is a real observation this contract does not get to deny.
- A completed instance produces `instance_not_suspendable` rather than a success.
- Each failure document carries exactly its discriminator and `instance_identifier` and proves no effect.
- A result naming another instance is refused.

- **Done when:** `cargo test -p slingshot-domain --test set_workflow_instance_suspension` proves both requested states, the observed-state answer, the not-suspendable refusal, and request-context validation.
