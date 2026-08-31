---
id: expose-asset-query-commands
title: "Expose Asset Query Commands"
workstream: "0026"
kind: task
depends_on:
  - expose-configuration-and-page-query-commands
gated: false
touches:
  - crates/slingshot-command-line/src/commands/asset_query.rs
  - crates/slingshot-command-line/tests/asset_query_commands.rs
status: done
merged_as: "b1331fe28b8b30196f76e8efaf2b68ca7982d057"
---
# Expose Asset Query Commands

Expose asset metadata filtering and page-reference lookup with typed byte ranges, media types, repeated tags, and repository paths.

**Steps:**

1. Commit fixtures for required metadata-query roots, absent and combined filters, ascending/permuted/duplicate media-format and tag flag sets, any-or-all tag matching, canonical typed property predicates, result windows, maximum/over-bound opaque continuation tokens, and referenced-page paths. Exercise the canonical unsigned base-ten asset-byte flag grammar and Plan 0003 `AssetByteLength` at zero, 9,223,372,036,854,775,807, the next integer, a negative integer, a leading-plus or leading-zero spelling, a fraction, an exponent, and parser overflow; reject invalid syntax/domain values before range-order comparison. Cover inclusive minimum/maximum ranges and an otherwise valid inverted range separately.
2. Reuse the exact shared predicate parser and expose `--offset` and `--limit`, or mutually exclusive `--continuation-token`, for both metadata and assets-used-by-page discovery commands.
3. Implement typed asset filter construction by rejecting duplicate format/tag flags and sorting each accepted set exactly once in ascending UTF-8 bytes before constructing the command. Preserve ordered property-predicate sequences separately; never serialize invocation order as an alternate asset-set spelling.
4. Implement the separate assets-used-by-page request and pass its Plan 0003 continuation token opaquely under the exact manifest bound.
5. Compare every canonical local request and help line with the exact `1.0.0` registry descriptor, limits/schema digests, continuation failures, and assert no publisher/tier option exists.

**Tests:**

- `asset_query_commands` covers the full filter matrix, shared predicate grammar, result window, exact-bound opaque continuation token, mutual exclusions, duplicate rejection, the exact canonical unsigned base-ten flag grammar and the matching `AssetByteLength` wire-integer domain, and range-order rejection after both endpoints are valid.
- Every accepted input permutation produces the same strictly ascending UTF-8 media-format/tag arrays; duplicates fail rather than collapse, and canonical request/digest fixtures never preserve flag order for these set-valued fields.
- A surface snapshot proves both asset query operations are present and tier selection is absent.

- **Done when:** `cargo test -p slingshot-command-line --test asset_query_commands` proves asset discovery constructs one canonical ascending format/tag request with exact bounded `AssetByteLength` endpoints, preserves ordered predicates, consumes exact registry identity, and accepts exactly one pagination mode while rejecting duplicates, malformed predicates, invalid or inverted byte ranges, and mixed continuation/window arguments.
