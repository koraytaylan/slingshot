---
id: create-authorizable
title: "Create a User or a Group"
workstream: "0049"
kind: task
depends_on:
  - authorizable-identity
  - resource-mutation
  - cancel-sling-job
gated: false
touches:
  - crates/slingshot-domain/src/command/create_authorizable.rs
  - crates/slingshot-domain/src/command/mod.rs
  - crates/slingshot-domain/tests/create_authorizable.rs
  - "crates/slingshot-domain/tests/fixtures/commands/create_authorizable/**"
status: planned
merged_as: ""
---
# Create a User or a Group

Creating a user and creating a group differ in one field and share every rule, so they land in one module as two commands. The rule that matters most is the one they share with the whole family: no credential crosses this boundary, so a created user has no password and cannot authenticate until an administrator supplies one through a channel this contract does not provide.

**Steps:**

1. Commit canonical accepted and refused argument fixtures and exact no-effect failure documents before the implementation, one line per vector, each carrying the note that says what it proves.
2. Implement `CreateUserCommand` and `CreateGroupCommand`, each with an `authorizable_identifier`, an optional `intermediate_path`, and an optional `properties` document under the existing mutation property model.
3. Accept no password, no key, and no token in either command, and prove that structurally rather than by review: the types have no member that could hold one.
4. Implement the shared result carrying the authorizable identifier and the repository address the author placed it at, which the request does not determine and therefore does not check.
5. Allow exactly `authorizable_already_exists`, `identifier_rejected`, `intermediate_path_rejected`, `property_rejected`, `authorizable_access_denied`, `repository_commit_failed`, and `mutation_outcome_unknown`.
6. Supply request-context validation that refuses a result naming another identifier.

**Tests:**

- Both commands round-trip byte-identically, are not interchangeable, and refuse each other's discriminator.
- A structural assertion proves neither command type nor the result type has a member that could carry a credential, and a secret sentinel placed in a profile property never appears in a rendered result or failure.
- The identifier and the intermediate path inherit every bound and refusal their values already prove.
- Each failure document carries exactly its discriminator and `authorizable_identifier` and proves no effect.
- A result naming another identifier is refused.

- **Done when:** `cargo test -p slingshot-domain --test create_authorizable` proves both commands, the structural absence of any credential member, the sentinel scan, every closed failure, and request-context validation.
