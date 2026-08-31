---
id: delete-authorizable
title: "Delete an Authorizable"
workstream: "0049"
kind: task
depends_on:
  - set-user-disabled
gated: false
touches:
  - crates/slingshot-domain/src/command/delete_authorizable.rs
  - crates/slingshot-domain/src/command/mod.rs
  - crates/slingshot-domain/tests/fixtures/command-module-inventory.txt
  - crates/slingshot-domain/tests/delete_authorizable.rs
  - "crates/slingshot-domain/tests/fixtures/commands/delete_authorizable/**"
status: done
merged_as: ""
---
# Delete an Authorizable

Removing the wrong authorizable is the mistake with the worst recovery in this family, so the command carries the kind it expects and refuses on mismatch. A guard is an argument here, not a convention.

**Steps:**

1. Commit canonical accepted and refused argument fixtures and exact no-effect failure documents before the implementation, one line per vector, each carrying the note that says what it proves.
2. Implement `DeleteAuthorizableCommand` with an `authorizable_identifier` and a required `expected_kind`.
3. Implement the result carrying the identifier, the kind that was removed, and the repository address it had.
4. Allow exactly `authorizable_not_found`, `authorizable_kind_mismatch`, `authorizable_access_denied`, `group_has_members`, `repository_commit_failed`, and `mutation_outcome_unknown`, refusing an absent authorizable.
5. Refuse removing a group that still has members under `group_has_members`, so emptying a group is a separate deliberate act rather than a side effect of deleting it.
6. Supply request-context validation that refuses a result naming another identifier or another kind than the expected one.

**Tests:**

- Every accepted vector round-trips byte-identically, and an absent `expected_kind` is refused.
- A user removed under an expected kind of group is refused with `authorizable_kind_mismatch`, and the reverse too.
- A group with members is refused with `group_has_members` rather than removed.
- An absent authorizable is a failure rather than a success with nothing to do.
- A result reporting a kind other than the expected one is refused.

- **Done when:** `cargo test -p slingshot-domain --test delete_authorizable` proves the kind guard in both directions, the non-empty-group refusal, the absent-target refusal, and request-context validation.
