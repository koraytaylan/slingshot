---
id: find-sling-jobs
title: "Find Sling Jobs"
workstream: "0048"
kind: task
depends_on:
  - list-sling-job-queues
gated: false
touches:
  - crates/slingshot-domain/src/command/find_sling_jobs.rs
  - crates/slingshot-domain/src/command/mod.rs
  - crates/slingshot-domain/tests/fixtures/command-module-inventory.txt
  - crates/slingshot-domain/tests/find_sling_jobs.rs
  - "crates/slingshot-domain/tests/fixtures/commands/find_sling_jobs/**"
status: planned
merged_as: ""
---
# Find Sling Jobs

Finding the jobs that failed on one topic is the question an operator asks after a queue tells them something is wrong. This task represents it as a windowed listing filtered by topic and state.

**Steps:**

1. Commit canonical accepted and refused argument fixtures and exact no-effect failure documents before the implementation, one line per vector, each carrying the note that says what it proves.
2. Implement `FindSlingJobsCommand` with an optional `topic`, a required non-empty ascending `states` set over the closed job state set, and an optional `result_window`.
3. Implement the match as the job identifier, the topic, the state, the queue name when the author reports one, and the retry count.
4. Order matches strictly ascending by job identifier, refusing a repeat.
5. Allow the shared discovery failures plus `job_inventory_failed`.
6. Supply request-context validation that refuses a match whose state is outside the requested set or whose topic is not the requested one.

**Tests:**

- An empty listing, a one-row listing, and a strictly ascending listing round-trip byte-identically.
- An empty state set is refused, a repeated state is refused, and a descending set is refused.
- A match in a state outside the requested set is refused, and every closed state appears across the fixtures.
- A match on another topic than the requested one is refused, while any topic is accepted when none was requested.
- An absent queue name is omitted rather than serialized as null.

- **Done when:** `cargo test -p slingshot-domain --test find_sling_jobs` proves the non-empty ascending state set, both request-context rules, every closed state, and every closed failure.
