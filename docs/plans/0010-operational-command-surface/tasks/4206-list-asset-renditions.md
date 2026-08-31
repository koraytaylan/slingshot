---
id: list-asset-renditions
title: "List Asset Renditions"
workstream: "0042"
kind: task
depends_on:
  - move-asset
  - operational-listing
gated: false
touches:
  - crates/slingshot-domain/src/command/list_asset_renditions.rs
  - crates/slingshot-domain/src/command/mod.rs
  - crates/slingshot-domain/tests/fixtures/command-module-inventory.txt
  - crates/slingshot-domain/tests/list_asset_renditions.rs
  - "crates/slingshot-domain/tests/fixtures/commands/list_asset_renditions/**"
status: done
merged_as: "cef06b2be13779e36f2cb62f0ed61e4168ca9dfa"
---
# List Asset Renditions

An asset's renditions are what a consumer actually fetches, and there is currently no way to ask which ones exist. This task represents one asset's renditions as a windowed listing ordered by rendition name.

**Steps:**

1. Commit canonical accepted and refused argument fixtures and exact no-effect failure documents before the implementation, one line per vector, each carrying the note that says what it proves.
2. Implement `ListAssetRenditionsCommand` with `asset_path` and an optional `result_window`.
3. Implement `RenditionMatch` carrying the rendition name bounded by `MAXIMUM_RENDITION_NAME_BYTES`, its repository address, its media type, and its byte length under `MAXIMUM_ASSET_BYTE_LENGTH`.
4. Order matches strictly ascending by rendition name under the shared text order rule, refusing a repeat.
5. Allow the shared discovery failures plus `asset_not_found`, `asset_access_denied`, and `asset_invalid`.
6. Supply request-context validation that refuses a match whose address is not under the requested asset.

**Tests:**

- An empty listing, a one-row listing, and a strictly ascending listing round-trip byte-identically.
- A repeated or descending rendition name is refused.
- The rendition name and byte length are each proved at their exact bound and one past it.
- A match addressed outside the requested asset is refused by request-context validation.
- Each failure document carries exactly its discriminator and `asset_path` and proves no effect.

- **Done when:** `cargo test -p slingshot-domain --test list_asset_renditions` proves the ordering rule, both sides of both bounds, the containment rule, and every closed failure.
