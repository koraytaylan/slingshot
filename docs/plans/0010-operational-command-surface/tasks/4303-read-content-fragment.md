---
id: read-content-fragment
title: "Read a Content Fragment"
workstream: "0043"
kind: task
depends_on:
  - create-content-fragment
gated: false
touches:
  - crates/slingshot-domain/src/command/read_content_fragment.rs
  - crates/slingshot-domain/src/command/mod.rs
  - crates/slingshot-domain/tests/fixtures/command-module-inventory.txt
  - crates/slingshot-domain/tests/read_content_fragment.rs
  - "crates/slingshot-domain/tests/fixtures/commands/read_content_fragment/**"
status: done
merged_as: ""
---
# Read a Content Fragment

`load_content_as_json` returns a fragment's storage, not its meaning: elements, variations, and the model they answer to are structure a caller would have to reconstruct. This task represents reading one fragment as what it is.

**Steps:**

1. Commit canonical accepted and refused argument fixtures and exact no-effect failure documents before the implementation, one line per vector, each carrying the note that says what it proves.
2. Implement `ReadContentFragmentCommand` with `fragment_path` and an optional `variation_name`.
3. Implement `ReadContentFragmentResult` carrying the fragment address, the model path, the title, the variation name that was read, and the elements in ascending name order, bounded by `MAXIMUM_OPERATIONAL_INSPECTION_RESULT_BYTES`.
4. Report the variation that was read even when the request named none, so a caller learns which one the master is.
5. Allow exactly `fragment_not_found`, `fragment_access_denied`, `fragment_invalid`, `variation_not_found`, and `result_budget_exceeded`.
6. Supply request-context validation that refuses a result naming another fragment or another variation than the one requested.

**Tests:**

- Every accepted vector round-trips byte-identically, with and without a requested variation.
- Elements are refused when unordered or repeated, and accepted when ascending.
- A result naming a variation other than the requested one is refused, while a result naming any variation is accepted when the request named none.
- The result budget is proved at its exact bound and one past it.
- Each failure document carries exactly its discriminator and `fragment_path` and proves no effect.

- **Done when:** `cargo test -p slingshot-domain --test read_content_fragment` proves the variation echo rule, the ascending elements, both sides of the result budget, and every closed failure.
