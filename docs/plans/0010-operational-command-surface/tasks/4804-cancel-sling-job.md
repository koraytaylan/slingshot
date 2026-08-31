---
id: cancel-sling-job
title: "Cancel a Sling Job"
workstream: "0048"
kind: task
depends_on:
  - inspect-sling-job
gated: false
touches:
  - crates/slingshot-domain/src/command/cancel_sling_job.rs
  - crates/slingshot-domain/src/command/mod.rs
  - crates/slingshot-domain/tests/cancel_sling_job.rs
  - "crates/slingshot-domain/tests/fixtures/commands/cancel_sling_job/**"
status: planned
merged_as: ""
---
# Cancel a Sling Job

A job retrying forever against something that will never succeed has to be stoppable, and stopping it is destructive in this plan's sense: work that was queued stops being queued.

**Steps:**

1. Commit canonical accepted and refused argument fixtures and exact no-effect failure documents before the implementation, one line per vector, each carrying the note that says what it proves.
2. Implement `CancelSlingJobCommand` with a `job_identifier` and nothing else.
3. Implement the result carrying the job identifier and the state observed after the cancellation.
4. Allow exactly `job_not_found`, `job_not_cancellable`, `platform_control_rejected`, and `platform_control_outcome_unknown`.
5. Supply request-context validation that refuses a result naming another job.

**Tests:**

- Every accepted vector round-trips byte-identically and refuses an unknown member.
- The observed state is a closed job state and an unknown spelling is refused.
- A job that already succeeded produces `job_not_cancellable` rather than a success.
- Each failure document carries exactly its discriminator and `job_identifier` and proves no effect.
- A result naming another job is refused.

- **Done when:** `cargo test -p slingshot-domain --test cancel_sling_job` proves the observed-state answer, the not-cancellable refusal, every closed failure, and request-context validation.
