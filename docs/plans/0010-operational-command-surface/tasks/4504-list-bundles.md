---
id: list-bundles
title: "List Bundles"
workstream: "0045"
kind: task
depends_on:
  - delete-configuration
  - platform-service-identity
gated: false
touches:
  - crates/slingshot-domain/src/command/list_open_service_gateway_initiative_bundles.rs
  - crates/slingshot-domain/src/command/mod.rs
  - crates/slingshot-domain/tests/list_open_service_gateway_initiative_bundles.rs
  - "crates/slingshot-domain/tests/fixtures/commands/list_open_service_gateway_initiative_bundles/**"
status: planned
merged_as: ""
---
# List Bundles

The first question after a deployment is which bundles are not active, and there is currently no way to ask it. This task represents the bundle inventory as a windowed listing with a state filter.

**Steps:**

1. Commit canonical accepted and refused argument fixtures and exact no-effect failure documents before the implementation, one line per vector, each carrying the note that says what it proves.
2. Implement the command with an optional `symbolic_name_prefix`, an optional `state` over the closed bundle state set, and an optional `result_window`.
3. Implement the match as symbolic name, version, state, and the author's own numeric bundle identifier.
4. Order matches strictly ascending by symbolic name, then by version, refusing a repeat of the pair.
5. Allow the shared discovery failures plus `bundle_inventory_failed`.
6. Supply request-context validation that refuses a match outside the requested prefix or state.

**Tests:**

- An empty listing, a one-row listing, and a strictly ascending listing round-trip byte-identically.
- Two bundles sharing a symbolic name and differing in version are ordered by version, and a repeat of the pair is refused.
- A match whose state is not the requested one is refused, and every closed state appears in the fixtures.
- A match outside the requested prefix is refused.
- Each failure document carries exactly its discriminator and proves no effect.

- **Done when:** `cargo test -p slingshot-domain --test list_open_service_gateway_initiative_bundles` proves the composite ordering, the prefix and state rules, the closed state set, and every closed failure.
