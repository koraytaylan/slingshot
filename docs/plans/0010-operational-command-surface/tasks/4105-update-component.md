---
id: update-component
title: "Update a Component"
workstream: "0041"
kind: task
depends_on:
  - list-child-pages
gated: false
touches:
  - crates/slingshot-domain/src/command/update_component.rs
  - crates/slingshot-domain/src/command/mod.rs
  - crates/slingshot-domain/tests/fixtures/command-module-inventory.txt
  - crates/slingshot-domain/tests/update_component.rs
  - "crates/slingshot-domain/tests/fixtures/commands/update_component/**"
status: done
merged_as: "f971744a47cdd3c277cb7473deccd7094e5a42c9"
---
# Update a Component

`add_component` puts a component on a page and nothing changes it afterwards. This task represents applying a property document and a bounded set of removals to one component resource addressed directly.

**Steps:**

1. Commit canonical accepted and refused argument fixtures and exact no-effect failure documents before the implementation, one line per vector, each carrying the note that says what it proves.
2. Implement `UpdateComponentCommand` with `component_path`, an optional `properties` document, and an optional bounded `removed_property_names` list.
3. Refuse a property named in both documents and refuse a request that changes nothing, under the same rules `update_page` states, reusing them rather than restating them.
4. Answer with the shared `ResourceMutationResult` carrying the component address.
5. Allow exactly `component_not_found`, `component_access_denied`, `component_invalid`, `property_rejected`, `property_not_removable`, `repository_commit_failed`, and `mutation_outcome_unknown`.
6. Supply request-context validation that refuses a result naming another component.

**Tests:**

- Every accepted vector round-trips byte-identically and answers with the requested address.
- The both-documents refusal and the empty-mutation refusal hold here and are proved against the shared rule rather than a copy of it.
- The removal list is proved at its exact bound and one past it.
- Each failure document carries exactly its discriminator and `component_path` and proves no effect.
- A result naming another component is refused.

- **Done when:** `cargo test -p slingshot-domain --test update_component` proves the shared mutation rules apply unchanged, both sides of the removal bound, every closed failure, and request-context validation.
