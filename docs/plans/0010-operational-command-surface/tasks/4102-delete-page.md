---
id: delete-page
title: "Delete a Page"
workstream: "0041"
kind: task
depends_on:
  - update-page
gated: false
touches:
  - crates/slingshot-domain/src/command/delete_page.rs
  - crates/slingshot-domain/src/command/mod.rs
  - crates/slingshot-domain/tests/fixtures/command-module-inventory.txt
  - crates/slingshot-domain/tests/delete_page.rs
  - "crates/slingshot-domain/tests/fixtures/commands/delete_page/**"
status: done
merged_as: "1523ff714eec589cb3ee19ab44a70979383ec9c9"
---
# Delete a Page

Removing a page is the operation an operator most wants a guard on. This task represents deleting one page and its subtree under a reference policy the caller states, and reporting how much was removed.

**Steps:**

1. Commit canonical accepted and refused argument fixtures and exact no-effect failure documents before the implementation, one line per vector, each carrying the note that says what it proves.
2. Implement `DeletePageCommand` with `page_path` and a required `reference_policy`, so neither refusing nor ignoring an incoming reference is a default somebody inherits without choosing it.
3. Implement `DeletePageResult` as the shared `DeletedResourceResult`: the removed address and a removed-node count bounded by `MAXIMUM_DELETED_NODES`.
4. Allow exactly `target_not_found`, `target_access_denied`, `target_not_a_page`, `target_is_referenced`, `deletion_budget_exceeded`, `repository_commit_failed`, and `mutation_outcome_unknown`. An absent target is a failure rather than a success with nothing to do.
5. Supply request-context validation that refuses a result whose removed address is not the requested page.

**Tests:**

- Every accepted vector round-trips byte-identically, and an absent `reference_policy` is refused.
- The removed-node count is accepted at exactly `MAXIMUM_DELETED_NODES` and refused one past it.
- `target_is_referenced` is reachable only under the refusing policy, and the fixture inventory proves both policies appear.
- Each failure document carries exactly its discriminator and `page_path` and proves no effect.
- A result naming another address is refused by request-context validation.

- **Done when:** `cargo test -p slingshot-domain --test delete_page` proves the required policy, both sides of the count bound, every closed failure including the absent-target refusal, and request-context validation.
