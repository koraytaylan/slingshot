---
id: update-content-fragment
title: "Update a Content Fragment"
workstream: "0043"
kind: task
depends_on:
  - read-content-fragment
gated: false
touches:
  - crates/slingshot-domain/src/command/update_content_fragment.rs
  - crates/slingshot-domain/src/command/mod.rs
  - crates/slingshot-domain/tests/update_content_fragment.rs
  - "crates/slingshot-domain/tests/fixtures/commands/update_content_fragment/**"
status: planned
merged_as: ""
---
# Update a Content Fragment

Editing a fragment is editing one variation of it, and a command that ignored variations would quietly write the master every time. This task represents applying a title and element values to one named variation.

**Steps:**

1. Commit canonical accepted and refused argument fixtures and exact no-effect failure documents before the implementation, one line per vector, each carrying the note that says what it proves.
2. Implement `UpdateContentFragmentCommand` with `fragment_path`, an optional `variation_name`, an optional `title`, and optional `elements`.
3. Refuse a request that carries neither a title nor an element, under the shared empty-mutation rule.
4. Answer with the shared `ResourceMutationResult` carrying the fragment address.
5. Allow exactly `fragment_not_found`, `fragment_access_denied`, `fragment_invalid`, `variation_not_found`, `element_unknown`, `element_value_rejected`, `repository_commit_failed`, and `mutation_outcome_unknown`.
6. Supply request-context validation that refuses a result naming another fragment.

**Tests:**

- Every accepted vector round-trips byte-identically, with and without a variation.
- The empty-mutation refusal holds, proved against the shared rule.
- Element values inherit every bound the shared element value proves.
- Each failure document carries exactly its discriminator and `fragment_path` and proves no effect.
- A result naming another fragment is refused.

- **Done when:** `cargo test -p slingshot-domain --test update_content_fragment` proves the empty-mutation refusal, the variation addressing, the inherited element bounds, every closed failure, and request-context validation.
