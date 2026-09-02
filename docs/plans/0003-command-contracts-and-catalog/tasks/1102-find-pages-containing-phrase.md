---
id: find-pages-containing-phrase
title: "Find Pages Containing a Phrase"
workstream: "0011"
kind: task
depends_on:
  - command-module-scaffold
  - repository-path
  - result-window
gated: false
touches:
  - crates/slingshot-domain/src/command/find_pages_containing_phrase.rs
  - crates/slingshot-domain/tests/fixtures/commands/find_pages_containing_phrase/**
  - crates/slingshot-domain/tests/find_pages_containing_phrase.rs
  - crates/slingshot-domain/src/command/query_paths.rs
status: done
merged_as: "803f73264c8bd02335fd48c268a9a513111fb690"
---
# Find Pages Containing a Phrase

Represent a bounded page full-text search rooted at an explicit repository path.

**Steps:**

1. Commit canonical command/result fixtures and language-neutral agent conformance scenarios for missing/inaccessible root anchors, exact `cq:Page`, nonpage, missing `jcr:content`, direct/descendant and single/multi-valued String, non-String, contiguous Unicode, case/normalization/stemming differences, leading/trailing/internal Unicode whitespace, title present/absent/wrong-type, offset/continuation, strictly ordered/results-byte completion, and all five common candidate/property-value/property-byte/criterion-evaluation/duration budgets before implementation.
2. Implement SearchPhrase as nonempty, control-free valid Unicode through its manifest bound. Reject an input whose first or last scalar has Unicode 15.1 `White_Space`; do not trim or normalize. Preserve every accepted byte and every internal whitespace scalar exactly.
3. Implement FindPagesContainingPhraseCommand with root, phrase, and ResultWindow.
4. Implement PageMatch with path and optional title, and implement the result in strict RepositoryPath byte order with optional next_continuation_token.
5. Preflight the root before enumeration. A missing or inaccessible root returns only closed no-effect `root_not_found` or `root_access_denied` with exactly `failure` and `root_path`, no matches, and no continuation token. Otherwise require exact `cq:Page`; scan JCR String values at/below direct `jcr:content` for one contiguous Unicode scalar sequence with no normalization/case folding/stemming/tokenization; derive optional title only from single direct String `jcr:title`, enforce the common computation budget without a partial page/token, and bind this plus continuation to semantic version.
6. Supply request-context validation that rejects a root-anchor failure unless `root_path` equals the originating command root and rejects cross-command result substitution.

**Tests:**

- Valid Unicode and internal-whitespace phrases round-trip exactly.
- Empty, whitespace-only, every leading/trailing Unicode 15.1 `White_Space`, control-containing, and over-bound phrase is rejected as noncanonical; accepted internal whitespace, combining sequences, normalization distinctions, and byte spelling are never trimmed or rewritten.
- Invalid root and result-window values reuse shared validation errors; valid-but-missing and valid-but-inaccessible roots use the exact external anchor failures.
- Empty and strictly ordered multi-page results serialize canonically, and continuation input/output obeys the shared contract.
- The result refuses duplicate page paths and unknown fields.
- Missing and inaccessible root fixtures preserve the validated request root in `root_path`, contain only that field plus their exact `root_not_found|root_access_denied` discriminator, reject unknown or surplus fields, prove enumeration did not begin, and expose neither matches nor a continuation token.
- Every one of the five common computation bounds uses the shared exact charge/check/tie rules and returns only closed DiscoveryBudgetExceeded with its exact discriminator.

- **Done when:** cargo test -p slingshot-domain --test find_pages_containing_phrase validates bounded phrase/search types, exact root-anchor failures, pagination/continuation, deterministic result order, and the complete exact language-neutral agent-conformance inventory without executing a repository search.
