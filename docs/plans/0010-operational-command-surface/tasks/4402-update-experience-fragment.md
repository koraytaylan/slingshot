---
id: update-experience-fragment
title: "Update an Experience Fragment"
workstream: "0044"
kind: task
depends_on:
  - create-experience-fragment
gated: false
touches:
  - crates/slingshot-domain/src/command/update_experience_fragment.rs
  - crates/slingshot-domain/src/command/mod.rs
  - crates/slingshot-domain/tests/fixtures/command-module-inventory.txt
  - crates/slingshot-domain/tests/update_experience_fragment.rs
  - "crates/slingshot-domain/tests/fixtures/commands/update_experience_fragment/**"
status: done
merged_as: "531272abad97284ebf36e3a12684145137339863"
---
# Update an Experience Fragment

An experience fragment's content lives in a variation, so this command addresses a variation directly rather than taking a fragment and a name and composing them a second way.

**Steps:**

1. Commit canonical accepted and refused argument fixtures and exact no-effect failure documents before the implementation, one line per vector, each carrying the note that says what it proves.
2. Implement `UpdateExperienceFragmentCommand` with `variation_path`, an optional `title`, an optional `properties` document, and an optional bounded `removed_property_names` list.
3. Refuse a property named in both documents and refuse a request that changes nothing, under the shared rules.
4. Answer with the shared `ResourceMutationResult` carrying the variation's content resource address.
5. Allow exactly `variation_not_found`, `variation_access_denied`, `variation_invalid`, `property_rejected`, `property_not_removable`, `repository_commit_failed`, and `mutation_outcome_unknown`.
6. Supply request-context validation that refuses a result naming another variation.

**Tests:**

- Every accepted vector round-trips byte-identically and computes the content-resource address the fixture states.
- The both-documents refusal and the empty-mutation refusal hold, proved against the shared rule.
- The removal list is proved at its exact bound and one past it.
- Each failure document carries exactly its discriminator and `variation_path` and proves no effect.
- A result naming another variation is refused.

- **Done when:** `cargo test -p slingshot-domain --test update_experience_fragment` proves the computed address, the shared mutation refusals, both sides of the removal bound, every closed failure, and request-context validation.
