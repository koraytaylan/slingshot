---
id: create-experience-fragment
title: "Create an Experience Fragment"
workstream: "0044"
kind: task
depends_on:
  - resource-mutation
  - delete-content-fragment
gated: false
touches:
  - crates/slingshot-domain/src/command/create_experience_fragment.rs
  - crates/slingshot-domain/src/command/mod.rs
  - crates/slingshot-domain/tests/fixtures/command-module-inventory.txt
  - crates/slingshot-domain/tests/create_experience_fragment.rs
  - "crates/slingshot-domain/tests/fixtures/commands/create_experience_fragment/**"
status: done
merged_as: ""
---
# Create an Experience Fragment

An experience fragment is a page-shaped thing with variations, and its first variation is created with it. Creating the container without one would leave a fragment nothing can render.

**Steps:**

1. Commit canonical accepted and refused argument fixtures and exact no-effect failure documents before the implementation, one line per vector, each carrying the note that says what it proves.
2. Implement `CreateExperienceFragmentCommand` with `parent_path`, a validated repository `name`, a `template_path`, an optional `title`, and a required `variation_name` bounded by `MAXIMUM_EXPERIENCE_FRAGMENT_VARIATION_NAME_BYTES`.
3. Compute both the fragment address and the variation address from the request, and answer with both, so a caller can address the variation immediately without guessing how the two compose.
4. Allow exactly `parent_not_found`, `parent_access_denied`, `target_already_exists`, `template_not_found`, `template_invalid`, `repository_commit_failed`, and `mutation_outcome_unknown`.
5. Supply request-context validation that refuses a result whose two addresses are not the computed ones.

**Tests:**

- Every accepted vector round-trips byte-identically and computes both addresses the fixture states.
- The variation name is proved at its bound on both sides, and an absent one is refused.
- The template path and the parent path stay distinguishable, and an invalid value for either names the right field.
- Each failure document carries exactly its discriminator and `target_path` and proves no effect.
- A result whose variation address is not under its fragment address is refused.

- **Done when:** `cargo test -p slingshot-domain --test create_experience_fragment` proves both computed addresses, their containment, both sides of the variation bound, every closed failure, and request-context validation.
