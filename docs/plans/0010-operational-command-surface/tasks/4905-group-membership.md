---
id: group-membership
title: "Add and Remove a Group Member"
workstream: "0049"
kind: task
depends_on:
  - delete-authorizable
gated: false
touches:
  - crates/slingshot-domain/src/command/group_membership.rs
  - crates/slingshot-domain/src/command/mod.rs
  - crates/slingshot-domain/tests/fixtures/command-module-inventory.txt
  - crates/slingshot-domain/tests/group_membership.rs
  - "crates/slingshot-domain/tests/fixtures/commands/group_membership/**"
status: done
merged_as: ""
---
# Add and Remove a Group Member

Adding and removing a membership are one relationship changed in two directions, and they answer the same question a caller has afterwards: did this change anything. Landing them together is what makes those two answers symmetric.

**Steps:**

1. Commit canonical accepted and refused argument fixtures and exact no-effect failure documents before the implementation, one line per vector, each carrying the note that says what it proves.
2. Implement `AddGroupMemberCommand` and `RemoveGroupMemberCommand`, each with a `group_identifier` and a `member_identifier`, and refuse a request whose two identifiers are equal, because a group cannot contain itself.
3. Implement the two results carrying both identifiers plus, respectively, whether the membership already existed and whether it existed at all, so a no-op is distinguishable from a change without a second request.
4. Allow exactly `group_not_found`, `member_not_found`, `authorizable_kind_mismatch`, `authorizable_access_denied`, `membership_cycle_refused`, `repository_commit_failed`, and `mutation_outcome_unknown` for both.
5. Refuse a membership that would make a group its own ancestor under `membership_cycle_refused`, which the author detects and this contract names.
6. Supply request-context validation that refuses a result echoing another request's pair.

**Tests:**

- Both commands round-trip byte-identically, are not interchangeable, and refuse a request whose identifiers are equal.
- Both results round-trip, and each carries exactly its own outcome member and refuses the other's.
- Each failure document carries exactly its discriminator and both identifiers, and proves no effect.
- A result echoing another pair is refused, in both directions.
- A member identifier naming a group is accepted, because a group may belong to a group, and the fixtures prove it.

- **Done when:** `cargo test -p slingshot-domain --test group_membership` proves both directions, the self-membership refusal, the distinguishable no-op answers, every closed failure, and request-context validation.
