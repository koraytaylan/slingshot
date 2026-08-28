---
id: dependency-direction-check
title: "Dependency Direction Check"
workstream: "0001"
kind: task
depends_on:
  - workspace-capability-probes
gated: false
touches:
  - crates/slingshot-development/src/dependency_direction.rs
  - crates/slingshot-development/src/main.rs
  - crates/slingshot-development/tests/dependency_direction.rs
  - "crates/slingshot-development/tests/fixtures/dependency-direction/**"
status: planned
merged_as: ""
---
# Dependency Direction Check

The crate diagram is an executable boundary: product adapters may depend inward on contracts, while command-line and daemon composition never become dependencies of lower layers.

**Steps:**

1. Write accepted, forbidden product edge, permitted product development-to-test-support, forbidden product normal/build-to-support, permitted test-support-to-inward-contract, forbidden test-support-to-outer-product, permitted outermost-development-to-product/support, forbidden dependency-on-development, forbidden storage-to-agent-protocol, permitted storage-to-domain durable-job vocabulary, and cyclic Cargo metadata fixtures independently from the checker implementation.
2. Implement the dependency-direction command over `cargo metadata --locked --format-version 1`, comparing direct local-package edges and dependency kinds with the architecture rules and detecting cycles.
3. Render one bounded diagnostic per forbidden edge with the dependent crate, dependency crate, dependency kind, and permitted direction; make the live-workspace check use the same pure evaluator as fixtures.
4. Extend the exhaustive development-binary dispatcher with `dependency-direction`, preserving the scaffold's existing metadata behavior and rejecting unknown commands, without making any product crate depend on development.

**Tests:**

- The accepted fixture and live workspace pass.
- Each forbidden fixture fails with the exact offending edge and no unrelated dependency.
- A product normal/build dependency on either support crate and any dependency on development fail even when acyclic; a product development dependency on test support and an inward development dependency pass only when the complete graph remains acyclic.
- Test support may depend only on the named inward product crates and cannot depend on configuration, agent connection, daemon, command line, or development; path-only executable values and reusable harnesses remain test-support-owned.
- Storage consumes domain-owned agent-job identity, state, sequence, and cursor values and has no agent-protocol edge; agent protocol converts wire values to those domain types.
- Development may depend inward on product and test support, but no local package may depend on development.
- A cycle reports the complete local-package cycle in deterministic order.
- Registry dependencies are ignored by the local-package direction table and remain covered by dependency policy instead.

- **Done when:** `cargo test -p slingshot-development --test dependency_direction` and `cargo run --locked -p slingshot-development -- dependency-direction` pass, while every forbidden fixture produces its pinned diagnostic.
