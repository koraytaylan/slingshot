---
id: operational-listing
title: "Operational Listing Page"
workstream: "0040"
kind: task
depends_on:
  - operational-contract-limits
gated: false
touches:
  - crates/slingshot-domain/src/command/operational_listing.rs
  - crates/slingshot-domain/src/command/mod.rs
  - crates/slingshot-domain/tests/fixtures/command-module-inventory.txt
  - crates/slingshot-domain/tests/operational_listing.rs
status: done
merged_as: "306545d6beb9bf51b5b7963cf7b4c3e9b5cff945"
---
# Operational Listing Page

Fourteen of the new reads return a page of rows that are not anchored anywhere in the repository: bundles, components, mappings, models, instances, queues, jobs, agents, queue entries, members. Plan 0003 already decided what a page is and how it resumes, and this task reuses that decision for rows keyed by text instead of by repository path rather than inventing a second pagination.

**Steps:**

1. Implement the strict ascending order rule over a text key, byte-wise, refusing a repeat and a descending pair, with a failure that names the offending key without echoing an unbounded value.
2. Implement `ListingResultFailure` with `NotStrictlyAscending` and `NotThisRequest`, mirroring the discovery failure that already exists so a reader meets one idea twice rather than two ideas once.
3. Reuse `ResultWindow` and `ContinuationToken` unchanged; declare no second offset, limit, or token type.
4. Declare the nonempty ascending requested-set shape here as well, once, rather than in each of the four families that asks which states a caller means. A set is checked against the order rather than sorted into it: sorting would accept two documents that mean the same thing and serialize differently, and the byte contract has no room for two.
5. Provide the shared constructor a listing result calls, which validates order before a value is built, so an unordered page cannot exist as a Rust value.

**Tests:**

- An empty page, a one-row page, and a strictly ascending page are accepted and round-trip byte-identically.
- A repeated key and a descending pair are both refused, and the failure names the key.
- Byte order rather than any locale order decides: a pair that differs only after a multi-byte scalar orders by its bytes and is proved against a fixture.
- A requested set refuses empty, refuses a repeat, refuses a descending pair, and is proved at its item bound and one member past it, with the bound checked before the order.
- A window round-trips in both its initial and its continuation form, and a continuation form beside an offset is still refused by the window it borrows.

- **Done when:** `cargo test -p slingshot-domain --test operational_listing` proves the order rule, both failures, and that no second window or token type was introduced.
