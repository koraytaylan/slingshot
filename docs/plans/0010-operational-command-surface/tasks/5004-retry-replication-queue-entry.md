---
id: retry-replication-queue-entry
title: "Retry a Replication Queue Entry"
workstream: "0050"
kind: task
depends_on:
  - flush-replication-queue
gated: false
touches:
  - crates/slingshot-domain/src/command/retry_replication_queue_entry.rs
  - crates/slingshot-domain/src/command/mod.rs
  - crates/slingshot-domain/tests/fixtures/command-module-inventory.txt
  - crates/slingshot-domain/tests/retry_replication_queue_entry.rs
  - "crates/slingshot-domain/tests/fixtures/commands/retry_replication_queue_entry/**"
status: planned
merged_as: ""
---
# Retry a Replication Queue Entry

The other half of a blocked queue: one entry that failed for a reason that has since been fixed, and no way to ask for it to be tried again without emptying everything behind it.

**Steps:**

1. Commit canonical accepted and refused argument fixtures and exact no-effect failure documents before the implementation, one line per vector, each carrying the note that says what it proves.
2. Implement `RetryReplicationQueueEntryCommand` with an `agent_identifier` and an `entry_identifier`.
3. Implement the result carrying both identifiers and whether the entry was resubmitted, so an entry that had already left the queue is distinguishable from one that was retried.
4. Allow exactly `agent_not_found`, `agent_access_denied`, `entry_not_found`, `platform_control_rejected`, and `platform_control_outcome_unknown`.
5. Supply request-context validation that refuses a result echoing another agent or another entry.

**Tests:**

- Every accepted vector round-trips byte-identically and keeps both identifiers distinguishable.
- Both outcomes appear in the fixtures, and the resubmitted member is required rather than optional.
- Each failure document carries exactly its discriminator and both identifiers, and proves no effect.
- A result echoing another pair is refused.
- Each identifier is proved at its exact bound and one past it.

- **Done when:** `cargo test -p slingshot-domain --test retry_replication_queue_entry` proves both outcomes, both sides of both identifier bounds, every closed failure, and request-context validation.
