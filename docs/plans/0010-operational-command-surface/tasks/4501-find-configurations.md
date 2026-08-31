---
id: find-configurations
title: "Find Configurations"
workstream: "0045"
kind: task
depends_on:
  - operational-listing
  - delete-experience-fragment
gated: false
touches:
  - crates/slingshot-domain/src/command/find_open_service_gateway_initiative_configurations.rs
  - crates/slingshot-domain/src/command/mod.rs
  - crates/slingshot-domain/tests/fixtures/command-module-inventory.txt
  - crates/slingshot-domain/tests/find_open_service_gateway_initiative_configurations.rs
  - "crates/slingshot-domain/tests/fixtures/commands/find_open_service_gateway_initiative_configurations/**"
status: planned
merged_as: ""
---
# Find Configurations

Inspecting a configuration requires knowing its exact persistent identifier, which is the thing an operator does not have. This task represents finding configurations by prefix and filter, and it deliberately reports no value: the metatype evidence that decides whether a value may be read is a per-identifier judgement, and a listing has not made it.

**Steps:**

1. Commit canonical accepted and refused argument fixtures and exact no-effect failure documents before the implementation, one line per vector, each carrying the note that says what it proves.
2. Implement the command with an optional `persistent_identifier_prefix`, an optional `filter` under the bounded lookup filter grammar the inspection command already defines, and an optional `result_window`, reusing that grammar rather than declaring a second one.
3. Implement the match as the persistent identifier, the factory persistent identifier when there is one, whether the configuration is bound to a bundle location, and the number of property keys it holds. No member carries a property value, and the type makes that structural rather than a promise.
4. Order matches strictly ascending by persistent identifier under the shared text order rule.
5. Allow the shared discovery failures plus `configuration_lookup_failed` and `configuration_lookup_budget_exceeded`.
6. Supply request-context validation that refuses a match whose identifier does not carry the requested prefix.

**Tests:**

- An empty listing, a one-row listing, and a strictly ascending listing round-trip byte-identically.
- A repeated or descending identifier is refused.
- The property-key count is proved at `MAXIMUM_INSPECTED_CONFIGURATION_PROPERTIES` and one past it.
- A match whose identifier does not carry the requested prefix is refused.
- A structural assertion proves the match type has no member that could hold a configuration value.
- Each failure document carries exactly its discriminator and proves no effect.

- **Done when:** `cargo test -p slingshot-domain --test find_open_service_gateway_initiative_configurations` proves the ordering rule, the prefix rule, both sides of the count bound, the absence of any value-bearing member, and every closed failure.
