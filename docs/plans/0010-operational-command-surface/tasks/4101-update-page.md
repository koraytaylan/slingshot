---
id: update-page
title: "Update a Page"
workstream: "0041"
kind: task
depends_on:
  - operational-contract-limits
  - resource-mutation
gated: false
touches:
  - crates/slingshot-domain/src/command/update_page.rs
  - crates/slingshot-domain/src/command/mod.rs
  - crates/slingshot-domain/tests/fixtures/command-module-inventory.txt
  - crates/slingshot-domain/tests/update_page.rs
  - "crates/slingshot-domain/tests/fixtures/commands/update_page/**"
status: done
merged_as: "1523ff714eec589cb3ee19ab44a70979383ec9c9"
---
# Update a Page

`create_page` has no counterpart: a page it created can be searched, packaged, and replicated, and never changed. This task represents applying a title, a property document, and a bounded set of property removals to one existing page's content resource, and answering with the address that changed.

**Steps:**

1. Commit canonical accepted and refused argument fixtures and exact no-effect failure documents before the implementation, one line per vector, each carrying the note that says what it proves.
2. Implement `UpdatePageCommand` with `page_path`, an optional `title`, an optional `properties` document under the existing JCR mutation property model, and an optional bounded `removed_property_names` list of validated property names, at most `MAXIMUM_REMOVED_PROPERTY_NAMES`.
3. Apply the shared property-mutation rule rather than restating it: a property named in both documents is refused rather than ordered, and a request carrying no title, no property, and no removal is refused, because a mutation that changes nothing is a caller believing something happened.
4. Compute the content-resource address from the page path rather than accepting one, reusing the content child name `create_page` already declares.
5. Answer with the shared `ResourceMutationResult`, and allow exactly `page_not_found`, `page_access_denied`, `page_invalid`, `property_rejected`, `property_not_removable`, `repository_commit_failed`, and `mutation_outcome_unknown`.
6. Supply request-context validation that refuses a result whose address is not the content resource this request determined, and refuses a cross-command substitution.

**Tests:**

- Every accepted vector round-trips byte-identically and computes the content-resource address the fixture states.
- A property named in both the property document and the removal list is refused, naming the property.
- A request with no title, no property, and no removal is refused.
- The removal list is accepted at exactly `MAXIMUM_REMOVED_PROPERTY_NAMES` and refused one past it, and a repeated name is refused.
- The property document inherits every bound and refusal the mutation property model already proves, without a second copy of any of them.
- Each failure document carries exactly its discriminator and `page_path`, rejects a surplus member, and proves no effect.
- A result naming another page's content resource is refused by request-context validation.

- **Done when:** `cargo test -p slingshot-domain --test update_page` proves the computed target, the both-documents refusal, the empty-mutation refusal, both sides of the removal bound, every closed failure, and request-context validation.
