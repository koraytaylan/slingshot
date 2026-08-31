---
id: replication-agents
title: "List and Inspect Replication Agents"
workstream: "0050"
kind: task
depends_on:
  - platform-service-identity
  - operational-listing
  - list-group-members
gated: false
touches:
  - crates/slingshot-domain/src/command/replication_agent.rs
  - crates/slingshot-domain/src/command/mod.rs
  - crates/slingshot-domain/tests/replication_agent.rs
  - "crates/slingshot-domain/tests/fixtures/commands/replication_agent/**"
status: planned
merged_as: ""
---
# List and Inspect Replication Agents

`replicate_content` offers content to replication and nothing can then ask what replication did with it. The two agent reads land together because they answer the same question at two depths, and because they share the one rule that matters: an agent's transport address carries its credentials, so neither reports it.

**Steps:**

1. Commit canonical accepted and refused argument fixtures and exact no-effect failure documents before the implementation, one line per vector, each carrying the note that says what it proves.
2. Implement `ListReplicationAgentsCommand` with an optional `result_window`, and `InspectReplicationAgentCommand` with an `agent_identifier`.
3. Implement the shared agent facts as the identifier, the agent's repository address, its title, whether it is enabled, its closed transport kind, whether its queue is blocked, and how many entries are queued; inspection adds the configured retry delay in milliseconds.
4. Carry no transport address, no user name, and no password. The types have no member that could hold one, so this is structural rather than a promise.
5. Order the listing strictly ascending by agent identifier, refusing a repeat.
6. Allow the shared discovery failures plus `agent_inventory_failed` for the listing, and `agent_not_found` and `agent_access_denied` for the inspection.
7. Supply request-context validation that refuses an inspection result naming another agent.

**Tests:**

- Both commands and both results round-trip byte-identically and are not interchangeable.
- A repeated or descending agent identifier is refused in the listing.
- A structural assertion proves neither result type has a member that could hold a transport address or a credential, and a sentinel placed in a fixture title never reaches a transport position.
- Every closed transport kind appears across the fixtures and an unknown spelling is refused.
- The queued-entry count is proved at `MAXIMUM_REPLICATION_QUEUE_ENTRIES` and one past it.

- **Done when:** `cargo test -p slingshot-domain --test replication_agent` proves both reads, the ordering rule, the structural absence of any transport address, every closed kind, and both sides of the count bound.
