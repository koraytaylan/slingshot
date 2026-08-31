---
id: delete-asset
title: "Delete an Asset"
workstream: "0042"
kind: task
depends_on:
  - update-asset-metadata
gated: false
touches:
  - crates/slingshot-domain/src/command/delete_asset.rs
  - crates/slingshot-domain/src/command/mod.rs
  - crates/slingshot-domain/tests/fixtures/command-module-inventory.txt
  - crates/slingshot-domain/tests/delete_asset.rs
  - "crates/slingshot-domain/tests/fixtures/commands/delete_asset/**"
status: done
merged_as: ""
---
# Delete an Asset

An asset is the thing most likely to be referenced by something an author cannot see from the asset itself, which is exactly why its deletion states a reference policy rather than assuming one.

**Steps:**

1. Commit canonical accepted and refused argument fixtures and exact no-effect failure documents before the implementation, one line per vector, each carrying the note that says what it proves.
2. Implement `DeleteAssetCommand` with `asset_path` and a required `reference_policy`.
3. Answer with the shared `DeletedResourceResult`.
4. Allow exactly `asset_not_found`, `asset_access_denied`, `asset_invalid`, `asset_is_referenced`, `deletion_budget_exceeded`, `repository_commit_failed`, and `mutation_outcome_unknown`, refusing an absent target.
5. Supply request-context validation that refuses a result whose removed address is not the requested asset.

**Tests:**

- Every accepted vector round-trips byte-identically, and an absent `reference_policy` is refused.
- The removed-node count is proved at its exact bound and one past it.
- `asset_is_referenced` is reachable only under the refusing policy, and both policies appear in the fixtures.
- Each failure document carries exactly its discriminator and `asset_path` and proves no effect.
- A result naming another address is refused.

- **Done when:** `cargo test -p slingshot-domain --test delete_asset` proves the required policy, both sides of the count bound, every closed failure including the absent-target refusal, and request-context validation.
