---
id: resolve-and-map-resource-path
title: "Resolve and Map a Resource Path"
workstream: "0046"
kind: task
depends_on:
  - list-resource-mappings
gated: false
touches:
  - crates/slingshot-domain/src/command/resource_resolution.rs
  - crates/slingshot-domain/src/command/mod.rs
  - crates/slingshot-domain/tests/fixtures/command-module-inventory.txt
  - crates/slingshot-domain/tests/resource_resolution.rs
  - "crates/slingshot-domain/tests/fixtures/commands/resource_resolution/**"
status: done
merged_as: "f7a356abc12e0d6fb065e195debd62b49b88d3ff"
---
# Resolve and Map a Resource Path

Two questions that share every value and answer opposite directions: which resource does this address reach, and which address reaches this resource. They land together because a shared trace is what makes the pair worth having, and two modules would give them two traces.

**Steps:**

1. Commit canonical accepted and refused argument fixtures and exact no-effect failure documents before the implementation, one line per vector, each carrying the note that says what it proves.
2. Implement `ResolveResourcePathCommand` with a `request_address` and an `include_trace` decision, and `ResolveResourcePathResult` carrying the resolved repository path when the address resolves, the resource type when there is one, and the selectors, extension, and suffix the resolution produced.
3. Implement `MapResourcePathCommand` with a `repository_path` and an optional `request_authority`, and `MapResourcePathResult` carrying the external address the author would emit.
4. Give both results the same ordered trace of entry addresses, bounded by `MAXIMUM_RESOLUTION_TRACE_ENTRIES`, present exactly when the request asked for it and refused otherwise, so the presence of a trace is the caller's decision rather than the author's.
5. Allow `resolution_failed` and `resolution_budget_exceeded` for both, and `request_address_rejected` for the resolution.
6. Supply request-context validation that refuses a resolution result echoing another address and a mapping result echoing another path.

**Tests:**

- Both commands and both results round-trip byte-identically, and the two results are not interchangeable.
- A trace present when none was asked for is refused, and an absent trace when one was asked for is accepted only when the resolution failed.
- The trace is proved at `MAXIMUM_RESOLUTION_TRACE_ENTRIES` and one past it.
- An address that does not resolve produces a result with no resolved path rather than a failure, and the documentation says why: not resolving is an answer.
- Each failure document carries exactly its discriminator and the value it names and proves no effect.

- **Done when:** `cargo test -p slingshot-domain --test resource_resolution` proves both directions, the trace presence rule in both directions, both sides of the trace bound, the unresolved-is-an-answer rule, and every closed failure.
