---
id: flush-replication-queue
title: "Flush a Replication Queue"
workstream: "0050"
kind: task
depends_on:
  - inspect-replication-queue
gated: false
touches:
  - crates/slingshot-domain/src/command/flush_replication_queue.rs
  - crates/slingshot-domain/src/command/mod.rs
  - crates/slingshot-domain/tests/fixtures/command-module-inventory.txt
  - crates/slingshot-domain/tests/flush_replication_queue.rs
  - "crates/slingshot-domain/tests/fixtures/commands/flush_replication_queue/**"
status: done
merged_as: "df21139d098315ffc10858d705ed7bbbe8ddb1b5"
---
# Flush a Replication Queue

Emptying a queue throws away work that was accepted, which is the most destructive thing in this family and the one an operator does under the most pressure. It therefore takes an optional expected count and refuses on mismatch, so a queue that grew between looking and acting is not silently emptied.

**Steps:**

1. Commit canonical accepted and refused argument fixtures and exact no-effect failure documents before the implementation, one line per vector, each carrying the note that says what it proves.
2. Implement `FlushReplicationQueueCommand` with an `agent_identifier` and an optional `expected_entry_count` bounded by `MAXIMUM_REPLICATION_QUEUE_ENTRIES`.
3. Implement the result carrying the agent identifier and the number of entries removed.
4. Allow exactly `agent_not_found`, `agent_access_denied`, `queue_expectation_mismatch`, `platform_control_rejected`, and `platform_control_outcome_unknown`.
5. Require the expectation to be checked before anything is removed, and state that the mismatch failure proves no effect, which is the whole reason the argument exists.
6. Supply request-context validation that refuses a result naming another agent, and one whose removed count exceeds a stated expectation.

**Tests:**

- Every accepted vector round-trips byte-identically, with and without an expectation.
- The expectation and the removed count are each proved at their exact bound and one past it.
- `queue_expectation_mismatch` carries exactly its discriminator and `agent_identifier`, and proves no effect.
- A result whose removed count exceeds a stated expectation is refused.
- A result naming another agent is refused.

- **Done when:** `cargo test -p slingshot-domain --test flush_replication_queue` proves the expectation guard, its no-effect failure, both sides of both bounds, and request-context validation.
