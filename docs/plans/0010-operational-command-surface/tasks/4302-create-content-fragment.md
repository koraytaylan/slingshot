---
id: create-content-fragment
title: "Create a Content Fragment"
workstream: "0043"
kind: task
depends_on:
  - content-fragment-elements
  - resource-mutation
gated: false
touches:
  - crates/slingshot-domain/src/command/create_content_fragment.rs
  - crates/slingshot-domain/src/command/mod.rs
  - crates/slingshot-domain/tests/create_content_fragment.rs
  - "crates/slingshot-domain/tests/fixtures/commands/create_content_fragment/**"
status: planned
merged_as: ""
---
# Create a Content Fragment

A content fragment is the unit headless consumers actually read, and nothing in the registry can make one. This task represents creating one under a model, with its initial element values.

**Steps:**

1. Commit canonical accepted and refused argument fixtures and exact no-effect failure documents before the implementation, one line per vector, each carrying the note that says what it proves.
2. Implement `CreateContentFragmentCommand` with `parent_path`, a validated repository `name`, a `model_path`, an optional `title`, and optional `elements`.
3. Compute the target address from parent and name, and answer with the shared `ResourceMutationResult`.
4. Allow exactly `parent_not_found`, `parent_access_denied`, `target_already_exists`, `model_not_found`, `model_invalid`, `element_unknown`, `element_value_rejected`, `repository_commit_failed`, and `mutation_outcome_unknown`.
5. State that an element the model does not declare is refused by the author rather than by this contract, which cannot know a model's elements, and give that refusal its own closed category so the caller can tell it from a value that was merely too long.
6. Supply request-context validation that refuses a result whose address is not the computed target.

**Tests:**

- Every accepted vector round-trips byte-identically and computes the target the fixture states.
- The model path and the parent path stay distinguishable in canonical JSON, and an invalid value for either names the right field.
- Element values inherit every bound the shared element value proves, without a second copy.
- Each failure document carries exactly its discriminator and `target_path` and proves no effect.
- A result naming another address is refused.

- **Done when:** `cargo test -p slingshot-domain --test create_content_fragment` proves the computed target, the two distinguishable paths, the inherited element bounds, every closed failure, and request-context validation.
