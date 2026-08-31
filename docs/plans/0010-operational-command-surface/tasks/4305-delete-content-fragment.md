---
id: delete-content-fragment
title: "Delete a Content Fragment"
workstream: "0043"
kind: task
depends_on:
  - update-content-fragment
gated: false
touches:
  - crates/slingshot-domain/src/command/delete_content_fragment.rs
  - crates/slingshot-domain/src/command/mod.rs
  - crates/slingshot-domain/tests/fixtures/command-module-inventory.txt
  - crates/slingshot-domain/tests/delete_content_fragment.rs
  - "crates/slingshot-domain/tests/fixtures/commands/delete_content_fragment/**"
status: done
merged_as: "37b2e8e9db8503286b3ba6df14ef0aeeb0015c67"
---
# Delete a Content Fragment

A fragment is referenced by every page that renders it, so its deletion states a reference policy for the reason an asset's does.

**Steps:**

1. Commit canonical accepted and refused argument fixtures and exact no-effect failure documents before the implementation, one line per vector, each carrying the note that says what it proves.
2. Implement `DeleteContentFragmentCommand` with `fragment_path` and a required `reference_policy`.
3. Answer with the shared `DeletedResourceResult`.
4. Allow exactly `fragment_not_found`, `fragment_access_denied`, `fragment_invalid`, `fragment_is_referenced`, `deletion_budget_exceeded`, `repository_commit_failed`, and `mutation_outcome_unknown`, refusing an absent target.
5. Supply request-context validation that refuses a result whose removed address is not the requested fragment.

**Tests:**

- Every accepted vector round-trips byte-identically, and an absent `reference_policy` is refused.
- The removed-node count is proved at its exact bound and one past it.
- `fragment_is_referenced` is reachable only under the refusing policy, and both policies appear in the fixtures.
- Each failure document carries exactly its discriminator and `fragment_path` and proves no effect.
- A result naming another address is refused.

- **Done when:** `cargo test -p slingshot-domain --test delete_content_fragment` proves the required policy, both sides of the count bound, every closed failure, and request-context validation.
