---
id: create-asset-folder
title: "Create an Asset Folder"
workstream: "0042"
kind: task
depends_on:
  - resource-mutation
  - reorder-component
gated: false
touches:
  - crates/slingshot-domain/src/command/create_asset_folder.rs
  - crates/slingshot-domain/src/command/mod.rs
  - crates/slingshot-domain/tests/fixtures/command-module-inventory.txt
  - crates/slingshot-domain/tests/create_asset_folder.rs
  - "crates/slingshot-domain/tests/fixtures/commands/create_asset_folder/**"
status: done
merged_as: "7e655ed4fdb2dd16c1a6367dd0661eb7ea63c02e"
---
# Create an Asset Folder

Assets can be searched and never written. The first thing a caller needs before writing one is somewhere to put it, which is one node with one primary type and one optional title - the smallest write in the family, and the one every other asset command assumes.

**Steps:**

1. Commit canonical accepted and refused argument fixtures and exact no-effect failure documents before the implementation, one line per vector, each carrying the note that says what it proves.
2. Implement `CreateAssetFolderCommand` with `parent_path`, a validated repository `name`, and an optional `title` bounded by `MAXIMUM_PAGE_TITLE_BYTES`, reusing the existing name and title values rather than declaring new ones.
3. Compute the target address from parent and name rather than accepting one, so the request determines it and the result echoes it.
4. Answer with the shared `ResourceMutationResult`.
5. Allow exactly `parent_not_found`, `parent_access_denied`, `target_already_exists`, `property_rejected`, `repository_commit_failed`, and `mutation_outcome_unknown`.
6. Supply request-context validation that refuses a result whose address is not the computed target.

**Tests:**

- Every accepted vector round-trips byte-identically and computes the target the fixture states, including under the repository root.
- A name the repository grammar refuses is refused here, naming the field.
- The title is proved at its exact bound and one past it.
- Each failure document carries exactly its discriminator and `target_path` and proves no effect.
- A result naming another address is refused.

- **Done when:** `cargo test -p slingshot-domain --test create_asset_folder` proves the computed target, both sides of the title bound, every closed failure, and request-context validation.
