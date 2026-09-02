---
id: inspect-replication-queue
title: "Inspect a Replication Queue"
workstream: "0050"
kind: task
depends_on:
  - replication-agents
gated: false
touches:
  - crates/slingshot-domain/src/command/inspect_replication_queue.rs
  - crates/slingshot-domain/src/command/mod.rs
  - crates/slingshot-domain/tests/fixtures/command-module-inventory.txt
  - crates/slingshot-domain/tests/inspect_replication_queue.rs
  - "crates/slingshot-domain/tests/fixtures/commands/inspect_replication_queue/**"
status: done
merged_as: "df21139d098315ffc10858d705ed7bbbe8ddb1b5"
---
# Inspect a Replication Queue

A blocked queue is the reason published content does not appear, and the entry at its head is the reason it is blocked. This task represents one agent's queue as a windowed listing whose page also carries the blocked state.

**Steps:**

1. Commit canonical accepted and refused argument fixtures and exact no-effect failure documents before the implementation, one line per vector, each carrying the note that says what it proves.
2. Implement `InspectReplicationQueueCommand` with an `agent_identifier` and an optional `result_window`.
3. Implement the entry as its identifier, the content path it carries, its closed replication action, its attempt count, and the last failure category it recorded when it has one.
4. Carry the queue's blocked state on the page itself rather than on every row, because it is a fact about the queue and repeating it per row would invite two answers.
5. Order entries strictly ascending by entry identifier, refusing a repeat, and bound the page by `MAXIMUM_REPLICATION_QUEUE_ENTRIES`.
6. Allow the shared discovery failures plus `agent_not_found`, `agent_access_denied`, and `queue_inventory_failed`.
7. Supply request-context validation that refuses a page whose entries are not strictly ascending.

**Tests:**

- An empty queue, a one-entry queue, and a strictly ascending queue round-trip byte-identically, blocked and not.
- A repeated or descending entry identifier is refused.
- Every closed replication action appears across the fixtures and an unknown spelling is refused.
- An absent last failure category is omitted rather than serialized as null.
- Each failure document carries exactly its discriminator and `agent_identifier` and proves no effect.

- **Done when:** `cargo test -p slingshot-domain --test inspect_replication_queue` proves the ordering rule, the page-level blocked state, every closed action, the omitted-rather-than-null member, and every closed failure.
