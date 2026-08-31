---
id: list-components
title: "List Declarative Service Components"
workstream: "0045"
kind: task
depends_on:
  - set-bundle-state
gated: false
touches:
  - crates/slingshot-domain/src/command/list_open_service_gateway_initiative_components.rs
  - crates/slingshot-domain/src/command/mod.rs
  - crates/slingshot-domain/tests/fixtures/command-module-inventory.txt
  - crates/slingshot-domain/tests/list_open_service_gateway_initiative_components.rs
  - "crates/slingshot-domain/tests/fixtures/commands/list_open_service_gateway_initiative_components/**"
status: planned
merged_as: ""
---
# List Declarative Service Components

A bundle can be active while the component that matters is unsatisfied, and that is the state an operator is usually hunting. This task represents the component inventory as a windowed listing with a state filter.

**Steps:**

1. Commit canonical accepted and refused argument fixtures and exact no-effect failure documents before the implementation, one line per vector, each carrying the note that says what it proves.
2. Implement the command with an optional `name_prefix`, an optional `state` over the closed component state set, and an optional `result_window`.
3. Implement the match as the component name, its declaring bundle's symbolic name, its state, and its service persistent identifier when it has one.
4. Order matches strictly ascending by component name, refusing a repeat.
5. Allow the shared discovery failures plus `component_inventory_failed`.
6. Supply request-context validation that refuses a match outside the requested prefix or state.

**Tests:**

- An empty listing, a one-row listing, and a strictly ascending listing round-trip byte-identically.
- A repeated or descending component name is refused.
- A match whose state is not the requested one is refused, and every closed state appears in the fixtures.
- An absent service persistent identifier is omitted rather than serialized as null.
- Each failure document carries exactly its discriminator and proves no effect.

- **Done when:** `cargo test -p slingshot-domain --test list_open_service_gateway_initiative_components` proves the ordering rule, the prefix and state rules, the omitted-rather-than-null member, and every closed failure.
