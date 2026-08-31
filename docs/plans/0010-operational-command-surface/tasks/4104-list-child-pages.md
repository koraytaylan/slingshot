---
id: list-child-pages
title: "List Child Pages"
workstream: "0041"
kind: task
depends_on:
  - move-page
  - operational-listing
gated: false
touches:
  - crates/slingshot-domain/src/command/list_child_pages.rs
  - crates/slingshot-domain/src/command/mod.rs
  - crates/slingshot-domain/tests/list_child_pages.rs
  - "crates/slingshot-domain/tests/fixtures/commands/list_child_pages/**"
status: planned
merged_as: ""
---
# List Child Pages

Every other page search descends a whole subtree. Walking a site one level at a time is the thing an operator actually does first, and doing it by filtering a subtree search is both slower and wrong at depth. This task represents listing the immediate child pages of one address.

**Steps:**

1. Commit canonical accepted and refused argument fixtures and exact no-effect failure documents before the implementation, one line per vector, each carrying the note that says what it proves.
2. Implement `ListChildPagesCommand` with `root_path` and an optional `result_window`, naming the anchor `root_path` so its anchor failure is the one every other rooted search already reports, with the same single field.
3. Define a match as a resource that is exactly a page and is an immediate child of the anchor. A grandchild does not match, and a non-page child does not match.
4. Reuse `PageMatch` and the strict ascending repository-path order rather than defining a second page row.
5. Allow the shared discovery failures and the shared root-anchor failures, and preflight the anchor before enumeration begins.
6. Supply request-context validation that refuses a match outside the anchor and refuses an anchor failure naming another request's root.

**Tests:**

- An empty listing, a one-row listing, and a strictly ascending listing round-trip byte-identically.
- A grandchild and a non-page child are proved not to match, against fixtures that contain both.
- Default and explicit result windows round-trip, and a continuation window beside an offset is refused.
- Missing and inaccessible anchors return only their closed failure with exactly `failure` and `root_path`, no matches, and no continuation token.
- Every one of the five shared computation budgets returns the closed budget failure with no partial page and no token.

- **Done when:** `cargo test -p slingshot-domain --test list_child_pages` proves immediate-child-only matching, the shared ordering and window rules, the exact anchor failures, and every budget boundary without executing a search.
