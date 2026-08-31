---
id: list-group-members
title: "List Group Members"
workstream: "0049"
kind: task
depends_on:
  - group-membership
  - operational-listing
gated: false
touches:
  - crates/slingshot-domain/src/command/list_group_members.rs
  - crates/slingshot-domain/src/command/mod.rs
  - crates/slingshot-domain/tests/fixtures/command-module-inventory.txt
  - crates/slingshot-domain/tests/list_group_members.rs
  - "crates/slingshot-domain/tests/fixtures/commands/list_group_members/**"
status: planned
merged_as: ""
---
# List Group Members

Membership is the question every permission question turns into, and answering it needs the difference between a direct member and one that arrives through another group. This task represents both, and says which is which.

**Steps:**

1. Commit canonical accepted and refused argument fixtures and exact no-effect failure documents before the implementation, one line per vector, each carrying the note that says what it proves.
2. Implement `ListGroupMembersCommand` with a `group_identifier`, a required `include_indirect` decision, and an optional `result_window`.
3. Implement the match as the member's identifier, its kind, its repository address, and whether the membership is direct.
4. Order matches strictly ascending by member identifier, refusing a repeat.
5. Allow the shared discovery failures plus `group_not_found`, `authorizable_kind_mismatch`, and `authorizable_access_denied`.
6. Supply request-context validation that refuses an indirect match when the request asked for direct members only.

**Tests:**

- An empty listing, a one-row listing, and a strictly ascending listing round-trip byte-identically.
- A repeated or descending member identifier is refused.
- An indirect match under a direct-only request is refused, and both decisions appear in the fixtures.
- A user identifier as the group produces `authorizable_kind_mismatch` rather than an empty listing.
- Each failure document carries exactly its discriminator and `group_identifier` and proves no effect.

- **Done when:** `cargo test -p slingshot-domain --test list_group_members` proves the ordering rule, the direct-only rule, the kind mismatch, and every closed failure.
