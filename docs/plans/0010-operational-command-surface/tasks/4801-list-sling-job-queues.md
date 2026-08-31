---
id: list-sling-job-queues
title: "List Sling Job Queues"
workstream: "0048"
kind: task
depends_on:
  - process-identity
  - operational-listing
  - set-workflow-instance-suspension
gated: false
touches:
  - crates/slingshot-domain/src/command/list_sling_job_queues.rs
  - crates/slingshot-domain/src/command/mod.rs
  - crates/slingshot-domain/tests/fixtures/command-module-inventory.txt
  - crates/slingshot-domain/tests/list_sling_job_queues.rs
  - "crates/slingshot-domain/tests/fixtures/commands/list_sling_job_queues/**"
status: planned
merged_as: ""
---
# List Sling Job Queues

A queue that is suspended or backed up explains every symptom downstream of it, and the queue inventory is the cheapest question in the family. It lands first because the other three are read through it.

**Steps:**

1. Commit canonical accepted and refused argument fixtures and exact no-effect failure documents before the implementation, one line per vector, each carrying the note that says what it proves.
2. Implement `ListSlingJobQueuesCommand` with an optional `result_window` and nothing else.
3. Implement the match as the queue name, its closed state, its active job count, and its queued job count, each count bounded by `MAXIMUM_OPERATIONAL_CANDIDATE_RECORDS`.
4. Order matches strictly ascending by queue name, refusing a repeat.
5. Allow the shared discovery failures plus `job_inventory_failed`.
6. Supply request-context validation that refuses a page whose queue names are not strictly ascending.

**Tests:**

- An empty listing, a one-row listing, and a strictly ascending listing round-trip byte-identically.
- A repeated or descending queue name is refused.
- Both counts are proved at their exact bound and one past it.
- Both closed queue states appear in the fixtures and an unknown spelling is refused.
- Each failure document carries exactly its discriminator and proves no effect.

- **Done when:** `cargo test -p slingshot-domain --test list_sling_job_queues` proves the ordering rule, both sides of both count bounds, the closed state set, and every closed failure.
