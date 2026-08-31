---
id: delete-component
title: "Delete a Component"
workstream: "0041"
kind: task
depends_on:
  - update-component
gated: false
touches:
  - crates/slingshot-domain/src/command/delete_component.rs
  - crates/slingshot-domain/src/command/mod.rs
  - crates/slingshot-domain/tests/fixtures/command-module-inventory.txt
  - crates/slingshot-domain/tests/delete_component.rs
  - "crates/slingshot-domain/tests/fixtures/commands/delete_component/**"
status: done
merged_as: ""
---
# Delete a Component

A component added by mistake currently has no way out. This task represents removing one component resource and the subtree it owns.

**Steps:**

1. Commit canonical accepted and refused argument fixtures and exact no-effect failure documents before the implementation, one line per vector, each carrying the note that says what it proves.
2. Implement `DeleteComponentCommand` with `component_path` and nothing else; a component is not referenced the way a page or an asset is, so this command states no reference policy rather than carrying one that would never apply.
3. Answer with the shared `DeletedResourceResult`, whose removed-node count is bounded by `MAXIMUM_DELETED_NODES`.
4. Allow exactly `component_not_found`, `component_access_denied`, `component_invalid`, `repository_commit_failed`, and `mutation_outcome_unknown`, refusing an absent target rather than reporting success.
5. Supply request-context validation that refuses a result whose removed address is not the requested component.

**Tests:**

- Every accepted vector round-trips byte-identically, and an unknown member is refused.
- The removed-node count is proved at its exact bound and one past it.
- An absent target is a failure, proved by a fixture that would otherwise read as a success.
- Each failure document carries exactly its discriminator and `component_path` and proves no effect.
- A result naming another address is refused.

- **Done when:** `cargo test -p slingshot-domain --test delete_component` proves the absent-target refusal, both sides of the count bound, every closed failure, and request-context validation.
