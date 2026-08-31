---
id: move-page
title: "Move a Page"
workstream: "0041"
kind: task
depends_on:
  - delete-page
gated: false
touches:
  - crates/slingshot-domain/src/command/move_page.rs
  - crates/slingshot-domain/src/command/mod.rs
  - crates/slingshot-domain/tests/fixtures/command-module-inventory.txt
  - crates/slingshot-domain/tests/move_page.rs
  - "crates/slingshot-domain/tests/fixtures/commands/move_page/**"
status: done
merged_as: "3fd7ff8550d67147d796fa3e9e752eab340cf192"
---
# Move a Page

Moving a page is two decisions: where it goes, and what happens to everything that pointed at it. This task represents both, and refuses the case that silently destroys a tree - a destination inside the source - before anything moves.

**Steps:**

1. Commit canonical accepted and refused argument fixtures and exact no-effect failure documents before the implementation, one line per vector, each carrying the note that says what it proves.
2. Implement `MovePageCommand` with `source_path`, `destination_path`, and a required `adjust_references` decision.
3. Refuse a destination equal to the source, a destination that is a descendant of the source, and a destination whose own parent is the source, before any other check, and give each its own closed failure.
4. Implement the result as the shared `MovedResourceResult`: both addresses and an adjusted-reference count bounded by `MAXIMUM_ADJUSTED_REFERENCES`.
5. Allow exactly `source_not_found`, `source_access_denied`, `destination_parent_not_found`, `destination_already_exists`, `destination_inside_source`, `reference_adjustment_budget_exceeded`, `repository_commit_failed`, and `mutation_outcome_unknown`.
6. Supply request-context validation that refuses a result whose source and destination are not this request's.

**Tests:**

- Every accepted vector round-trips byte-identically and keeps the two addresses distinguishable in canonical JSON.
- A destination equal to, inside, or immediately under the source is refused with `destination_inside_source`, proved on the exact boundary where the destination is the source's own child.
- The adjusted-reference count is accepted at its exact limit and refused one past it, and is zero when references are not adjusted.
- Each failure document carries exactly its discriminator and the address it names, and proves no effect.
- A result echoing another request's source or destination is refused.

- **Done when:** `cargo test -p slingshot-domain --test move_page` proves the containment refusals at their boundary, both sides of the reference bound, every closed failure, and request-context validation.
