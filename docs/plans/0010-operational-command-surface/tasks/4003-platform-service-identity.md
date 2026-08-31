---
id: platform-service-identity
title: "Platform Service Identity"
workstream: "0040"
kind: task
depends_on:
  - operational-contract-limits
gated: false
touches:
  - crates/slingshot-domain/src/command/platform_service_identity.rs
  - crates/slingshot-domain/src/command/mod.rs
  - crates/slingshot-domain/tests/platform_service_identity.rs
status: planned
merged_as: ""
---
# Platform Service Identity

The platform and replication families address bundles, declarative service components, replication agents, and queue entries. Each has a grammar the author already enforces, and a caller that sends something outside it should learn so here rather than from a remote parse failure that says less.

**Steps:**

1. Implement `BundleSymbolicName` under the Open Service Gateway Initiative token grammar: one or more full-stop-separated tokens, each non-empty and drawn from letters, digits, hyphen-minus, and low line, bounded by `MAXIMUM_BUNDLE_SYMBOLIC_NAME_BYTES`.
2. Implement `BundleVersion` as major, minor, and micro unsigned integers with an optional qualifier over letters, digits, hyphen-minus, and low line, bounded by `MAXIMUM_BUNDLE_VERSION_BYTES`, refusing a leading zero in a numeric segment and a fifth segment.
3. Implement `DeclarativeServiceComponentName` as a bounded non-empty name refusing controls and edge spaces.
4. Implement `ReplicationAgentIdentifier` and `ReplicationQueueEntryIdentifier` as bounded non-empty opaque values refusing controls and edge spaces, at their own named limits.
5. Implement the closed `BundleState`, `ComponentState`, `ReplicationTransportKind`, and `ReplicationAction` enumerations the architecture names, each serialized in snake case.

**Tests:**

- Each identifier accepts its ordinary spellings and refuses empty, oversized by one byte, control-bearing, and space-edged forms, with the exact bound proved on both sides.
- The symbolic name refuses an empty token, a leading or trailing full stop, and a character outside its alphabet.
- The version accepts three and four segments, refuses two, five, a leading zero, and a qualifier outside its alphabet, and round-trips its exact spelling.
- Every closed enumeration round-trips through canonical JSON and refuses an unknown spelling, and each has exactly the members the architecture lists.

- **Done when:** `cargo test -p slingshot-domain --test platform_service_identity` proves every grammar, both sides of every bound, and the closed membership of all four enumerations.
