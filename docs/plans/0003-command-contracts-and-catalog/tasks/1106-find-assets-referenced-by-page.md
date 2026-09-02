---
id: find-assets-referenced-by-page
title: "Find Assets Referenced by a Page"
workstream: "0011"
kind: task
depends_on:
  - command-module-scaffold
  - repository-path
  - result-window
  - search-predicates
gated: false
touches:
  - crates/slingshot-domain/src/command/find_assets_referenced_by_page.rs
  - crates/slingshot-domain/tests/fixtures/commands/find_assets_referenced_by_page/**
  - crates/slingshot-domain/tests/find_assets_referenced_by_page.rs
status: done
merged_as: "d7cbdaf94a4a2ef98f3f792d68ce4299ce02a2c8"
---
# Find Assets Referenced by a Page

Represent discovery of assets referenced from one page and the page properties that establish each reference.

**Steps:**

1. Commit canonical command/result fixtures and language-neutral agent conformance scenarios for missing/inaccessible/not-a-page anchors, exact `cq:Page`/missing content, single/multi JCR Path/String references, invalid string, missing/non-`dam:Asset` target, direct/nested referring property paths, repeated/multiple assets, offset/continuation, strictly ordered/results-byte completion, and all five common candidate/property-value/property-byte/criterion-evaluation/duration budgets before implementation.
2. Implement FindAssetsReferencedByPageCommand with page RepositoryPath and ResultWindow.
3. Implement AssetReferenceMatch with `dam:Asset` RepositoryPath and a non-empty deduplicated UTF-8-ordered collection of RelativePropertyPath values rooted at page `jcr:content`.
4. Implement the result with asset paths and nested reference paths strictly ascending by validated byte spelling, optional next_continuation_token, and no duplicate asset match.
5. Preflight the page before scanning its content. A missing, inaccessible, or accessible non-`cq:Page` anchor returns only closed no-effect `page_not_found`, `page_access_denied`, or `page_invalid` with exactly `failure` and `page_path`, no matches, and no continuation token. Otherwise define a reference as one JCR Path or String value at/below page `jcr:content` that validates as absolute RepositoryPath and resolves to exact `dam:Asset`; invalid/missing/nonasset targets do not match, and common computation-budget exhaustion returns no partial aggregation/token.
6. Supply request-context validation that rejects a page-anchor failure unless `page_path` equals the originating command page and rejects cross-command result substitution.

**Tests:**

- Page and asset path validation use the shared path contract.
- Each asset match requires at least one unique reference path.
- Independently authored scenarios require repeated references to one asset to remain one match with ordered unique reference paths; Rust validates that result invariant without scanning a repository.
- Empty and paginated/continued results serialize canonically under the shared continuation contract.
- Duplicate assets, duplicate reference paths, and unknown fields are rejected.
- Missing, inaccessible, and accessible non-page anchor fixtures preserve the validated request page in `page_path`, contain only that field plus their exact `page_not_found|page_access_denied|page_invalid` discriminator, reject unknown or surplus fields, prove scanning did not begin, and expose neither matches nor a continuation token.
- Every one of the five common budget boundaries uses the shared exact charge/check/tie rules and yields only closed DiscoveryBudgetExceeded without partial aggregation/token.

- **Done when:** cargo test -p slingshot-domain --test find_assets_referenced_by_page validates exact page-anchor failures, aggregation/result invariants, deterministic ordering, pagination/continuation, and the complete pinned language-neutral reference-discovery conformance inventory.
