---
id: update-configuration
title: "Update a Configuration"
workstream: "0045"
kind: task
depends_on:
  - find-configurations
gated: false
touches:
  - crates/slingshot-domain/src/command/update_open_service_gateway_initiative_configuration.rs
  - crates/slingshot-domain/src/command/mod.rs
  - crates/slingshot-domain/tests/fixtures/command-module-inventory.txt
  - crates/slingshot-domain/tests/update_open_service_gateway_initiative_configuration.rs
  - "crates/slingshot-domain/tests/fixtures/commands/update_open_service_gateway_initiative_configuration/**"
status: done
merged_as: ""
---
# Update a Configuration

Changing a configuration is the platform action an operator takes most often and the one most likely to carry a secret. Values go in because that is the point of the command; nothing comes back out but counts.

**Steps:**

1. Commit canonical accepted and refused argument fixtures and exact no-effect failure documents before the implementation, one line per vector, each carrying the note that says what it proves.
2. Implement the command with an exact `persistent_identifier`, an `assignments` document of typed scalars and bounded sequences under the property model the inspection command already maps, and an optional bounded `removed_property_keys` list.
3. Refuse a key named in both documents, and refuse a request that assigns and removes nothing.
4. Implement the result carrying the persistent identifier and the number of keys changed, and nothing else. No value, no before-and-after, no echo of an assignment.
5. Allow exactly `configuration_lookup_failed`, `configuration_lookup_mismatch`, `configuration_lookup_ambiguous`, `configuration_value_unsupported`, `configuration_value_malformed`, `platform_control_rejected`, and `platform_control_outcome_unknown`.
6. Supply request-context validation that refuses a result naming another persistent identifier.

**Tests:**

- Every accepted vector round-trips byte-identically and inherits every value bound the inspection property model already proves.
- A key in both documents is refused, and an empty change is refused.
- The changed-key count is proved at its exact bound and one past it.
- A structural assertion proves the result type has no member that could hold an assigned value, and a secret sentinel placed in an assignment never appears in any rendered result or failure.
- Each failure document carries exactly its discriminator and `persistent_identifier` and proves no effect.

- **Done when:** `cargo test -p slingshot-domain --test update_open_service_gateway_initiative_configuration` proves the both-documents and empty-change refusals, the count bound, the structural absence of any echoed value, the sentinel scan, and every closed failure.
