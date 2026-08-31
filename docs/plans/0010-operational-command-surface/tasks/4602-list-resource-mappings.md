---
id: list-resource-mappings
title: "List Resource Mappings"
workstream: "0046"
kind: task
depends_on:
  - resource-mapping-entries
gated: false
touches:
  - crates/slingshot-domain/src/command/list_resource_mappings.rs
  - crates/slingshot-domain/src/command/mod.rs
  - crates/slingshot-domain/tests/fixtures/command-module-inventory.txt
  - crates/slingshot-domain/tests/list_resource_mappings.rs
  - "crates/slingshot-domain/tests/fixtures/commands/list_resource_mappings/**"
status: done
merged_as: ""
---
# List Resource Mappings

Nobody can reason about a resolution problem without seeing the entries that decide it, and reading them out of the repository by hand means knowing where a deployment put them. This task represents the effective mapping as a windowed listing.

**Steps:**

1. Commit canonical accepted and refused argument fixtures and exact no-effect failure documents before the implementation, one line per vector, each carrying the note that says what it proves.
2. Implement `ListResourceMappingsCommand` with an optional `result_window` and nothing else, because the mapping is one inventory and filtering it by pattern would invite a caller to believe a pattern was matched rather than listed.
3. Order entries strictly ascending by entry address, refusing a repeat, and reuse `ResourceMappingEntry` unchanged.
4. Allow the shared discovery failures plus `mapping_inventory_failed`.
5. Supply request-context validation that refuses a page whose entries are not strictly ascending.

**Tests:**

- An empty listing, a one-row listing, and a strictly ascending listing round-trip byte-identically.
- A repeated or descending entry address is refused.
- Every closed kind appears across the fixtures, including a redirect with a status code and a map without one.
- Default and explicit result windows round-trip, and a continuation window beside an offset is refused.
- Each failure document carries exactly its discriminator and proves no effect.

- **Done when:** `cargo test -p slingshot-domain --test list_resource_mappings` proves the ordering rule, every closed kind, the window rules, and every closed failure.
