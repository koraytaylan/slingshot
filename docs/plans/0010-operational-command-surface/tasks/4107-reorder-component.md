---
id: reorder-component
title: "Reorder a Component"
workstream: "0041"
kind: task
depends_on:
  - delete-component
gated: false
touches:
  - crates/slingshot-domain/src/command/reorder_component.rs
  - crates/slingshot-domain/src/command/mod.rs
  - crates/slingshot-domain/tests/fixtures/command-module-inventory.txt
  - crates/slingshot-domain/tests/reorder_component.rs
  - "crates/slingshot-domain/tests/fixtures/commands/reorder_component/**"
status: done
merged_as: ""
---
# Reorder a Component

`add_component` appends last and says so. Ordering is what a component's position on a page means, so a surface that can only append is a surface that can only build a page in one order. This task represents moving one component within its orderable parent.

**Steps:**

1. Commit canonical accepted and refused argument fixtures and exact no-effect failure documents before the implementation, one line per vector, each carrying the note that says what it proves.
2. Implement `ReorderComponentCommand` with `component_path` and a closed `placement` of either `before` carrying a validated `sibling_name` or `last` carrying nothing.
3. Refuse a placement naming the component's own name as the sibling, because a component cannot precede itself.
4. Answer with the component address and the name it now follows, absent when it is now first.
5. Allow exactly `component_not_found`, `component_access_denied`, `parent_not_orderable`, `sibling_not_found`, `repository_commit_failed`, and `mutation_outcome_unknown`, reusing the orderable-parent failure `add_component` already defines.
6. Supply request-context validation that refuses a result naming another component.

**Tests:**

- Both placement forms round-trip byte-identically and neither accepts the other's members.
- A `before` placement naming the component itself is refused.
- A result reporting a following name when the placement was `last`, or reporting one that is the component itself, is refused.
- Each failure document carries exactly its discriminator and `component_path` and proves no effect.
- A result naming another component is refused.

- **Done when:** `cargo test -p slingshot-domain --test reorder_component` proves both closed placements, the self-sibling refusal, the answer's following-name rule, every closed failure, and request-context validation.
