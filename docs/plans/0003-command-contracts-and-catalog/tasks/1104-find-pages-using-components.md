---
id: find-pages-using-components
title: "Find Pages Using Components"
workstream: "0011"
kind: task
depends_on:
  - command-module-scaffold
  - find-pages-containing-phrase
  - repository-path
  - result-window
gated: false
touches:
  - crates/slingshot-domain/src/command/find_pages_using_components.rs
  - crates/slingshot-domain/tests/fixtures/commands/find_pages_using_components/**
  - crates/slingshot-domain/tests/find_pages_using_components.rs
status: planned
merged_as: ""
---
# Find Pages Using Components

Represent finding pages that use any or all of a non-empty set of component resource types.

**Steps:**

1. Commit canonical command/result fixtures and language-neutral agent conformance scenarios for missing/inaccessible root anchors, exact `cq:Page`, nonpage/missing content, content-root/descendant resource type, non-String/multi type, no super-type expansion, Any/All distributed components, duplicate/empty/over-bound, offset/continuation, strictly ordered/results-byte completion, and all five common candidate/property-value/property-byte/criterion-evaluation/duration budgets before implementation.
2. Reuse foundation-owned ComponentResourceType with its distinct bounded absolute-or-relative slash-separated NFC Sling grammar and no JCR same-name-sibling or namespace-colon interpretation.
3. Implement ComponentMatchMode as Any or All and FindPagesUsingComponentsCommand with root, a non-empty deduplicated resource-type set, match mode, and ResultWindow.
4. Preserve caller order for resource types while rejecting duplicates.
5. Preflight the root before enumeration. A missing or inaccessible root returns only closed no-effect `root_not_found` or `root_access_denied` with exactly `failure` and `root_path`, no matches, and no continuation token. Otherwise treat `jcr:content` and every descendant with one direct JCR String `sling:resourceType` as component resources, compare complete strings without super-type expansion, define All across the whole page rather than one resource, reuse PageMatch, and enforce shared ordering/continuation plus common computation budgets without partial output.
6. Supply request-context validation that rejects a root-anchor failure unless `root_path` equals the originating command root and rejects cross-command result substitution.

**Tests:**

- Any and All have distinct canonical request fixtures.
- Empty, duplicate, malformed-segment, dot/traversal, colon/bracket/wildcard/pipe, non-NFC, and over-bound resource-type collections are rejected independently of JCR-name rules.
- Resource-type order survives round-trip serialization.
- Shared path and result-window boundaries remain enforced.
- Empty and strictly ordered result pages reject duplicate page paths and unknown fields; continuation input/output obeys the shared contract.
- Missing and inaccessible root fixtures preserve the validated request root in `root_path`, contain only that field plus their exact `root_not_found|root_access_denied` discriminator, reject unknown or surplus fields, prove enumeration did not begin, and expose neither matches nor a continuation token.
- Every one of the five common budget boundaries uses the shared exact charge/check/tie rules and returns closed DiscoveryBudgetExceeded and no partial result/token.

- **Done when:** cargo test -p slingshot-domain --test find_pages_using_components validates resource-type/match-mode collections, exact root-anchor failures, deterministic result ordering, pagination/continuation, and the complete language-neutral component-matching conformance inventory without executing a repository search.
