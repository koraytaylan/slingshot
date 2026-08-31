---
id: delete-configuration
title: "Delete a Configuration"
workstream: "0045"
kind: task
depends_on:
  - update-configuration
gated: false
touches:
  - crates/slingshot-domain/src/command/delete_open_service_gateway_initiative_configuration.rs
  - crates/slingshot-domain/src/command/mod.rs
  - crates/slingshot-domain/tests/delete_open_service_gateway_initiative_configuration.rs
  - "crates/slingshot-domain/tests/fixtures/commands/delete_open_service_gateway_initiative_configuration/**"
status: planned
merged_as: ""
---
# Delete a Configuration

Removing a configuration restores whatever the code defaults to, which is a state change an operator needs to make deliberately and be told about exactly.

**Steps:**

1. Commit canonical accepted and refused argument fixtures and exact no-effect failure documents before the implementation, one line per vector, each carrying the note that says what it proves.
2. Implement the command with an exact `persistent_identifier` and nothing else.
3. Implement the result carrying the persistent identifier and whether the configuration was bound to a factory, which is the one fact that changes what deletion means.
4. Allow exactly `configuration_lookup_failed`, `configuration_lookup_mismatch`, `configuration_lookup_ambiguous`, `platform_control_rejected`, and `platform_control_outcome_unknown`, refusing an absent configuration.
5. Supply request-context validation that refuses a result naming another persistent identifier.

**Tests:**

- Every accepted vector round-trips byte-identically and refuses an unknown member.
- An absent configuration is a failure rather than a success with nothing to do.
- The persistent identifier is proved at its exact bound and one past it.
- Each failure document carries exactly its discriminator and `persistent_identifier` and proves no effect.
- A result naming another identifier is refused.

- **Done when:** `cargo test -p slingshot-domain --test delete_open_service_gateway_initiative_configuration` proves the absent-target refusal, both sides of the identifier bound, every closed failure, and request-context validation.
