---
id: expose-page-mutation-commands
title: "Expose Page Mutation Commands"
workstream: "0026"
kind: task
depends_on:
  - define-command-invocations
gated: false
touches:
  - crates/slingshot-command-line/src/commands/page_mutation.rs
  - crates/slingshot-command-line/src/property_document.rs
  - crates/slingshot-command-line/tests/page_mutation_commands.rs
status: planned
merged_as: ""
---
# Expose Page Mutation Commands

Expose page creation and component addition with bounded duplicate-free property documents and explicit mutation classification.

**Steps:**

1. Commit property-document and invocation fixtures for page parent, name, title, and template; component page, relative parent resource, name, and resource type; nested values; duplicate keys; size/depth bounds; malformed paths; caller operation keys; and every missing required value.
2. Implement bounded property reading plus the two typed registry request mappings without templating or shell interpretation, requiring a caller key from their non-intrinsically-idempotent registry classification before file or daemon access.
3. Pin request bytes, mutation labels, and diagnostics for every fixture. Require both exact `1.0.0` descriptor identities and the canonical limits/schema digests; assert add-component exposes all eight authoritative-no-effect categories including `parent_not_orderable` plus `mutation_outcome_unknown`, with no CLI alias.

**Tests:**

- `page_mutation_commands` proves exact page/component requests, all required identity and placement fields, and property values.
- Duplicate, oversized, overdeep, and nonobject property documents fail before daemon submission.
- Missing caller keys fail before property-file or daemon access for both non-intrinsically-idempotent operations.
- Descriptor coverage fails if `parent_not_orderable` is absent/renamed, any registered add-component failure field changes, or a stale version/limits digest is bound.

- **Done when:** `cargo test -p slingshot-command-line --test page_mutation_commands` proves page/component mutations require caller keys before access and submit only bounded typed property objects.
