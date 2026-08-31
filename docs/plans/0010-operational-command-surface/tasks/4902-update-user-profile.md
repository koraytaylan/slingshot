---
id: update-user-profile
title: "Update a User Profile"
workstream: "0049"
kind: task
depends_on:
  - create-authorizable
gated: false
touches:
  - crates/slingshot-domain/src/command/update_user_profile.rs
  - crates/slingshot-domain/src/command/mod.rs
  - crates/slingshot-domain/tests/fixtures/command-module-inventory.txt
  - crates/slingshot-domain/tests/update_user_profile.rs
  - "crates/slingshot-domain/tests/fixtures/commands/update_user_profile/**"
status: done
merged_as: "a73a0b157a88358ff0b1aa98aff3ea0fee48c32e"
---
# Update a User Profile

A profile is ordinary properties on a user's profile resource, and the only thing that makes it worth its own command is that the resource address is computed from an identifier rather than given as a path.

**Steps:**

1. Commit canonical accepted and refused argument fixtures and exact no-effect failure documents before the implementation, one line per vector, each carrying the note that says what it proves.
2. Implement `UpdateUserProfileCommand` with an `authorizable_identifier`, an optional `properties` document, and an optional bounded `removed_property_names` list.
3. Refuse a property named in both documents and refuse a request that changes nothing, under the shared rules.
4. Implement the result carrying the identifier and the profile resource address the author reports.
5. Allow exactly `authorizable_not_found`, `authorizable_kind_mismatch`, `authorizable_access_denied`, `property_rejected`, `property_not_removable`, `repository_commit_failed`, and `mutation_outcome_unknown`.
6. Supply request-context validation that refuses a result naming another identifier.

**Tests:**

- Every accepted vector round-trips byte-identically.
- The both-documents refusal and the empty-mutation refusal hold, proved against the shared rule.
- The removal list is proved at its exact bound and one past it.
- A group identifier produces `authorizable_kind_mismatch` rather than a success, proved by a fixture.
- A secret sentinel placed in a profile property never appears in a rendered result or failure.

- **Done when:** `cargo test -p slingshot-domain --test update_user_profile` proves the shared mutation refusals, the kind mismatch, both sides of the removal bound, and request-context validation.
