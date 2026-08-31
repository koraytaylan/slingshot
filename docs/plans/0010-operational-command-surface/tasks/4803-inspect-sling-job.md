---
id: inspect-sling-job
title: "Inspect a Sling Job"
workstream: "0048"
kind: task
depends_on:
  - find-sling-jobs
gated: false
touches:
  - crates/slingshot-domain/src/command/inspect_sling_job.rs
  - crates/slingshot-domain/src/command/mod.rs
  - crates/slingshot-domain/tests/fixtures/command-module-inventory.txt
  - crates/slingshot-domain/tests/inspect_sling_job.rs
  - "crates/slingshot-domain/tests/fixtures/commands/inspect_sling_job/**"
status: done
merged_as: "5f8dabf0bc80ff1bf0a55195ce88cfc3957e9f9e"
---
# Inspect a Sling Job

A job's own properties are what an operator wants and are also where a deployment puts whatever it likes, including things it should not. This task reports the keys and never the values, for the reason the configuration listing reports none.

**Steps:**

1. Commit canonical accepted and refused argument fixtures and exact no-effect failure documents before the implementation, one line per vector, each carrying the note that says what it proves.
2. Implement `InspectSlingJobCommand` with a `job_identifier` and nothing else.
3. Implement the result carrying the job identifier, topic, state, queue name when there is one, retry count, maximum retry count, and the job's property keys in ascending order at most `MAXIMUM_SLING_JOB_PROPERTY_KEYS`.
4. Carry no property value. The type has no member that could hold one, so this is structural rather than a promise.
5. Allow exactly `job_not_found`, `job_inventory_failed`, and `result_budget_exceeded`.
6. Supply request-context validation that refuses a result naming another job.

**Tests:**

- Every accepted vector round-trips byte-identically, with an empty key list and with a full one.
- The key list is proved at `MAXIMUM_SLING_JOB_PROPERTY_KEYS` and one past it, and a repeated or descending key is refused.
- A structural assertion proves the result type has no member that could hold a property value, and a secret sentinel placed in a fixture key never reaches a rendered value position.
- The result budget is proved at its exact bound and one past it.
- A result naming another job is refused.

- **Done when:** `cargo test -p slingshot-domain --test inspect_sling_job` proves the ascending bounded keys, the structural absence of values, both sides of the result budget, and every closed failure.
