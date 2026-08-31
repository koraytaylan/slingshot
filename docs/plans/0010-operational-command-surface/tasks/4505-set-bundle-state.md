---
id: set-bundle-state
title: "Set a Bundle State"
workstream: "0045"
kind: task
depends_on:
  - list-bundles
gated: false
touches:
  - crates/slingshot-domain/src/command/set_open_service_gateway_initiative_bundle_state.rs
  - crates/slingshot-domain/src/command/mod.rs
  - crates/slingshot-domain/tests/fixtures/command-module-inventory.txt
  - crates/slingshot-domain/tests/set_open_service_gateway_initiative_bundle_state.rs
  - "crates/slingshot-domain/tests/fixtures/commands/set_open_service_gateway_initiative_bundle_state/**"
status: done
merged_as: "6a823953ac8e4e7cf5a603243fec21ffc0a53361"
---
# Set a Bundle State

Starting, stopping, or refreshing a bundle changes no content and is plainly not a read, which is the case that made this plan widen what access means. The command answers with the state it observed rather than the state it asked for.

**Steps:**

1. Commit canonical accepted and refused argument fixtures and exact no-effect failure documents before the implementation, one line per vector, each carrying the note that says what it proves.
2. Implement the command with a `symbolic_name` and a closed `transition` of `start`, `stop`, or `refresh`.
3. Implement the result carrying the symbolic name and the state observed after the transition, so a transition that was accepted and did not take effect is visible rather than reported as success.
4. Allow exactly `bundle_not_found`, `bundle_transition_refused`, `platform_control_rejected`, and `platform_control_outcome_unknown`.
5. Supply request-context validation that refuses a result naming another symbolic name.

**Tests:**

- Every accepted vector round-trips byte-identically, and each of the three transitions appears in the fixtures.
- The observed state is a closed bundle state and an unknown spelling is refused.
- A result reporting a state after `stop` that is `active` is accepted rather than refused, and the documentation says why: this contract reports what the author observed and does not decide whether the author is wrong.
- Each failure document carries exactly its discriminator and `symbolic_name` and proves no effect.
- A result naming another bundle is refused.

- **Done when:** `cargo test -p slingshot-domain --test set_open_service_gateway_initiative_bundle_state` proves all three transitions, the observed-state answer, every closed failure, and request-context validation.
