---
id: move-asset
title: "Move an Asset"
workstream: "0042"
kind: task
depends_on:
  - delete-asset
gated: false
touches:
  - crates/slingshot-domain/src/command/move_asset.rs
  - crates/slingshot-domain/src/command/mod.rs
  - crates/slingshot-domain/tests/fixtures/command-module-inventory.txt
  - crates/slingshot-domain/tests/move_asset.rs
  - "crates/slingshot-domain/tests/fixtures/commands/move_asset/**"
status: done
merged_as: ""
---
# Move an Asset

Moving an asset breaks every page that referred to it unless the references move with it, so the decision about references is an argument here for the same reason it is one on a page move.

**Steps:**

1. Commit canonical accepted and refused argument fixtures and exact no-effect failure documents before the implementation, one line per vector, each carrying the note that says what it proves.
2. Implement `MoveAssetCommand` with `source_path`, `destination_path`, and a required `adjust_references` decision.
3. Refuse a destination equal to, inside, or immediately under the source before any other check, reusing the rule `move_page` states rather than restating it.
4. Answer with the shared `MovedResourceResult`.
5. Allow exactly `source_not_found`, `source_access_denied`, `destination_parent_not_found`, `destination_already_exists`, `destination_inside_source`, `reference_adjustment_budget_exceeded`, `repository_commit_failed`, and `mutation_outcome_unknown`.
6. Supply request-context validation that refuses a result whose addresses are not this request's.

**Tests:**

- Every accepted vector round-trips byte-identically and keeps both addresses distinguishable.
- The containment refusal is proved at its boundary, against the shared rule rather than a copy of it.
- The adjusted-reference count is proved at its exact bound and one past it, and is zero when references are not adjusted.
- Each failure document carries exactly its discriminator and the address it names and proves no effect.
- A result echoing another request's addresses is refused.

- **Done when:** `cargo test -p slingshot-domain --test move_asset` proves the shared containment rule, both sides of the reference bound, every closed failure, and request-context validation.
