---
id: query-paths
title: "Query Paths"
workstream: "0011"
kind: task
depends_on:
  - command-module-scaffold
  - repository-path
  - result-window
  - search-predicates
gated: false
touches:
  - crates/slingshot-domain/src/command/query_paths.rs
  - crates/slingshot-domain/tests/fixtures/commands/query_paths/**
  - crates/slingshot-domain/tests/query_paths.rs
status: done
merged_as: "ae7c90545a935b658164b86122fb76e57f195fe9"
---
# Query Paths

Represent a bounded structured repository path query without exposing raw query-language execution.

**Steps:**

1. Commit canonical command/result fixtures and language-neutral agent conformance scenarios for root inclusion, missing/inaccessible root anchors, descendant node, exact/missing primary type, direct/nested/missing RelativePropertyPath, every predicate operator/type mismatch, combined logical-And, offset/continuation, strictly ordered/results-byte completion, and every common candidate/property-value/property-byte/criterion-evaluation/duration budget before implementation.
2. Implement PrimaryNodeTypeName as a separately bounded qualified RepositoryName without same-name-sibling syntax and QueryPathsCommand with root, optional primary node type, predicate collection, and ResultWindow.
3. Bound the predicate collection with the shared named limit.
4. Implement PathMatch and QueryPathsResult with matches strictly ascending by RepositoryPath bytes and optional next_continuation_token.
5. Preflight the root before enumeration. A missing or inaccessible root returns only the closed no-effect `root_not_found` or `root_access_denied` object with exactly `failure` and `root_path`; it carries no matches or continuation token. Otherwise define candidates as every JCR node at/below root in strict order, primary type as exact qualified `jcr:primaryType`, relative paths from each candidate, missing path as false for every operator including NotEquals, no scalar/list/type coercion, logical And, and continuation semantics; enforce the common computation budget and discard every accumulated match/token on DiscoveryBudgetExceeded.
6. Supply request-context validation that rejects a root-anchor failure unless `root_path` equals the originating command root and rejects any result variant for another command.

**Tests:**

- Every supported request combination has an exact canonical fixture.
- Invalid root, node type, predicate count, and result window errors name the failing field; valid-but-missing and valid-but-inaccessible roots instead produce their exact external anchor failures.
- Offset and continuation inputs obey the shared mutual-exclusion rule; next tokens round-trip unchanged and bind to the same non-window arguments.
- Independently authored scenarios pin root/descendant matching and logical-And behavior; Rust validates their command/result documents and strict RepositoryPath byte order without executing a repository query.
- Duplicate result paths and unknown fields are rejected.
- Strings that contain query syntax remain predicate values and are not parsed as a query.
- Missing/type-mismatched predicate targets and missing primary types follow exact nonmatch fixtures in both Adobe Experience Manager deployment eras.
- Missing and inaccessible root fixtures return exactly `{"failure":"root_not_found","root_path":...}` or `{"failure":"root_access_denied","root_path":...}` with the validated request root, reject unknown or surplus fields, perform no enumeration, and expose neither matches nor a continuation token.
- All five common budget discriminators have exact charge-unit/order, below/at/above, duration/page-completion tie, checked-overflow, and cancellation vectors; exhaustion yields no partial matches or continuation token.

- **Done when:** cargo test -p slingshot-domain --test query_paths validates bounded structured-query types, exact root-anchor failures, deterministic result ordering/continuation, raw-query noninterpretation, and the complete pinned language-neutral agent-conformance inventory.
