---
id: update-asset-metadata
title: "Update Asset Metadata"
workstream: "0042"
kind: task
depends_on:
  - create-asset
gated: false
touches:
  - crates/slingshot-domain/src/command/update_asset_metadata.rs
  - crates/slingshot-domain/src/command/mod.rs
  - crates/slingshot-domain/tests/fixtures/command-module-inventory.txt
  - crates/slingshot-domain/tests/update_asset_metadata.rs
  - "crates/slingshot-domain/tests/fixtures/commands/update_asset_metadata/**"
status: done
merged_as: "7e655ed4fdb2dd16c1a6367dd0661eb7ea63c02e"
---
# Update Asset Metadata

Asset metadata is what `find_assets_by_metadata` searches, and until now nothing could write it. This task represents applying a property document and a bounded set of removals to one asset's metadata resource.

**Steps:**

1. Commit canonical accepted and refused argument fixtures and exact no-effect failure documents before the implementation, one line per vector, each carrying the note that says what it proves.
2. Implement `UpdateAssetMetadataCommand` with `asset_path`, an optional `properties` document, and an optional bounded `removed_property_names` list.
3. Compute the metadata resource address from the asset path rather than accepting one, and declare the child names it is composed from beside the command.
4. Refuse a property named in both documents and refuse a request that changes nothing, under the shared rules.
5. Answer with the shared `ResourceMutationResult` carrying the metadata resource address.
6. Allow exactly `asset_not_found`, `asset_access_denied`, `asset_invalid`, `property_rejected`, `property_not_removable`, `repository_commit_failed`, and `mutation_outcome_unknown`.

**Tests:**

- Every accepted vector round-trips byte-identically and computes the metadata address the fixture states.
- The both-documents refusal and the empty-mutation refusal hold, proved against the shared rule.
- The removal list is proved at its exact bound and one past it.
- Each failure document carries exactly its discriminator and `asset_path` and proves no effect.
- A result naming another asset's metadata resource is refused.

- **Done when:** `cargo test -p slingshot-domain --test update_asset_metadata` proves the computed metadata address, the shared mutation refusals, both sides of the removal bound, every closed failure, and request-context validation.
