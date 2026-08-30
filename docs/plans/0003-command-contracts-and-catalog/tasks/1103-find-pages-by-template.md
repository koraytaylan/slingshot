---
id: find-pages-by-template
title: "Find Pages by Template"
workstream: "0011"
kind: task
depends_on:
  - command-module-scaffold
  - find-pages-containing-phrase
  - repository-path
  - result-window
gated: false
touches:
  - crates/slingshot-domain/src/command/find_pages_by_template.rs
  - crates/slingshot-domain/tests/fixtures/commands/find_pages_by_template/**
  - crates/slingshot-domain/tests/find_pages_by_template.rs
status: done
merged_as: ""
---
# Find Pages by Template

Represent finding pages below one root whose template path equals one validated repository path.

**Steps:**

1. Commit canonical command/result fixtures and language-neutral agent conformance scenarios for missing/inaccessible root anchors, exact `cq:Page`, nonpage, missing content/template, single JCR Path/String template, multi/wrong type, exact/nonexact template, offset/continuation, empty/strictly ordered/results-byte completion, and all five common candidate/property-value/property-byte/criterion-evaluation/duration budgets before implementation.
2. Implement FindPagesByTemplateCommand with distinct root and template RepositoryPath fields and ResultWindow.
3. Reuse PageMatch without defining a second page result representation.
4. Implement the strict RepositoryPath byte-ordered result and optional next_continuation_token.
5. Preflight the root before enumeration. A missing or inaccessible root returns only closed no-effect `root_not_found` or `root_access_denied` with exactly `failure` and `root_path`, no matches, and no continuation token. Otherwise define a match as exact `cq:Page` at/below root whose single direct `jcr:content/cq:template` JCR Path or String validates as and exactly equals the requested RepositoryPath; enforce the common computation budget without partial matches/token and reject missing/multi/wrong types, unknown fields, unordered paths, and duplicates.
6. Supply request-context validation that rejects a root-anchor failure unless `root_path` equals the originating command root and rejects cross-command result substitution.

**Tests:**

- Root and template paths remain distinguishable in canonical JSON.
- Invalid values for either path report the correct field.
- Default and explicit result windows round-trip.
- Empty and strictly ordered result pages preserve their exact shape, and continuation input/output obeys the shared contract.
- Duplicate matches and unknown fields are rejected.
- Missing and inaccessible root fixtures preserve the validated request root in `root_path`, contain only that field plus their exact `root_not_found|root_access_denied` discriminator, reject unknown or surplus fields, prove enumeration did not begin, and expose neither matches nor a continuation token.
- Every one of the five common budget boundaries uses the shared exact charge/check/tie rules and returns closed DiscoveryBudgetExceeded with no partial result/token.

- **Done when:** cargo test -p slingshot-domain --test find_pages_by_template validates request distinction, bounds, exact root-anchor failures, pagination/continuation, deterministic result documents, and the complete exact-template agent-conformance inventory without executing a repository search.
