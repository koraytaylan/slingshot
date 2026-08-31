---
id: resource-mapping-entries
title: "Resource Mapping Entries"
workstream: "0046"
kind: task
depends_on:
  - operational-listing
  - list-components
gated: false
touches:
  - crates/slingshot-domain/src/command/resource_mapping_entry.rs
  - crates/slingshot-domain/src/command/mod.rs
  - crates/slingshot-domain/tests/fixtures/command-module-inventory.txt
  - crates/slingshot-domain/tests/resource_mapping_entry.rs
  - "crates/slingshot-domain/tests/fixtures/commands/resource_mapping_entry/**"
status: done
merged_as: ""
---
# Resource Mapping Entries

The three mapping commands share one row shape and one closed kind. Landing them once keeps a listing and a trace describing the same entry the same way, which is the whole point of being able to compare them.

**Steps:**

1. Commit canonical accepted and refused argument fixtures and exact no-effect failure documents before the implementation, one line per vector, each carrying the note that says what it proves.
2. Implement `ResourceMappingPattern` as a bounded non-empty value at `MAXIMUM_RESOURCE_MAPPING_PATTERN_BYTES`, refusing controls and edge spaces, and treating the pattern as opaque text: this contract does not parse a regular expression and does not claim to.
3. Implement `ResourceMappingKind` as the closed `map`, `internal_redirect`, `redirect`, or `alias`.
4. Implement `ResourceMappingEntry` carrying the entry's repository address, its pattern, its kind, its ordered replacements bounded by `MAXIMUM_RESOURCE_MAPPING_REPLACEMENTS`, and its status code, present only when the kind is `redirect` and refused otherwise.
5. Implement `RequestAddress` as a bounded absolute request address at `MAXIMUM_REQUEST_ADDRESS_BYTES`, refusing controls, whitespace, and a form with no scheme or no path.

**Tests:**

- Each bound is accepted exactly and refused one past it, for the pattern, the replacement list, and the request address.
- A status code present on a non-redirect kind is refused, and an absent status code on a redirect kind is refused.
- Every closed kind round-trips and an unknown spelling is refused.
- An empty replacement list is refused for kinds that redirect or map and accepted for none.
- The request address refuses a relative form, a control, and embedded whitespace.

- **Done when:** `cargo test -p slingshot-domain --test resource_mapping_entry` proves every bound on both sides, the closed kind, and the status-code rule in both directions.
