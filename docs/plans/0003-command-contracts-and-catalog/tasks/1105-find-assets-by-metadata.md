---
id: find-assets-by-metadata
title: "Find Assets by Metadata"
workstream: "0011"
kind: task
depends_on:
  - command-module-scaffold
  - repository-path
  - result-window
  - search-predicates
gated: false
touches:
  - crates/slingshot-domain/src/command/find_assets_by_metadata.rs
  - crates/slingshot-domain/tests/fixtures/commands/find_assets_by_metadata/**
  - crates/slingshot-domain/tests/find_assets_by_metadata.rs
status: done
merged_as: "31371867beb9e16c7cf4041ec2e9742a0f4834e1"
---
# Find Assets by Metadata

Represent a bounded asset search across media format, byte-size range, tags, and typed metadata predicates.

**Steps:**

1. Commit canonical command/result fixtures and language-neutral agent conformance scenarios for missing/inaccessible root anchors, exact `dam:Asset`/nonasset, primary/fallback/missing/wrong-type media format, original rendition binary/missing/aggregate-size trap, AssetByteLength zero/maximum/next/negative/plus/leading-zero/fraction/exponent/overflow and request/result parity, single/multi/missing tags, canonical/permuted/duplicate request sets, nested metadata predicate/missing/type mismatch, each criterion/logical-And combination, inclusive boundaries, offset/continuation, strictly ordered/results-byte completion, and all five common candidate/property-value/property-byte/criterion-evaluation/duration budgets before implementation.
2. Implement MediaFormat and AssetTag as bounded non-empty values, TagMatchMode as Any or All, and shared AssetByteLength as one JSON integer token with exact minimal `0|[1-9][0-9]*` spelling and domain zero through manifest-owned MAXIMUM_ASSET_BYTE_LENGTH, the largest nonnegative signed-64-bit JCR Binary length. Reject strings, plus, leading zero, negative/negative-zero, fraction, exponent, next-unit, and checked overflow rather than coercing them.
3. Implement AssetByteRange with optional AssetByteLength minimum and maximum values and reject a minimum greater than its maximum while accepting equality at zero and the maximum.
4. Implement FindAssetsByMetadataCommand with root, media formats, byte range, tags, tag match mode, property predicates, and ResultWindow. Public typed constructors reject duplicates and sort each accepted media-format/tag set once in ascending UTF-8 bytes before constructing the value. Wire deserialization requires both arrays to be already strictly ascending and unique and rejects a merely permuted representation; the canonical arrays participate in command and continuation-query digests.
5. Implement AssetMatch with path, optional resolved media format, optional original-rendition AssetByteLength using the identical canonical JSON integer representation, and sorted tags; implement FindAssetsByMetadataResult with matches in strict RepositoryPath order and optional next_continuation_token. Property predicates filter candidates and do not add an implicit metadata projection.
6. Preflight the root before enumeration. A missing or inaccessible root returns only closed no-effect `root_not_found` or `root_access_denied` with exactly `failure` and `root_path`, no matches, and no continuation token. Otherwise pin asset semantics to exact `dam:Asset`; ordered format fallback; original `jcr:data` Binary length only; `cq:tags`; predicates relative to asset node; missing-value nonmatch when its criterion is requested; logical And across families; and common computation-budget failure with no partial output/token.
7. Supply request-context validation that rejects a root-anchor failure unless `root_path` equals the originating command root and rejects cross-command result substitution.

**Tests:**

- Every individual and combined criterion has an exact canonical request fixture.
- Invalid and over-bound format, tag, predicate, and collection cases are rejected.
- Typed constructors produce one ascending UTF-8 byte spelling regardless of input permutation after rejecting duplicates; deserialization rejects duplicate, descending, and otherwise permuted media-format/tag arrays rather than silently rewriting signed wire bytes.
- AssetByteLength zero and MAXIMUM_ASSET_BYTE_LENGTH round-trip identically in request ranges and AssetMatch; maximum-plus-one, negative including negative zero, plus, leading-zero, fraction, exponent, string, and parse overflow fail at their exact canonical-byte/schema/typed stage. Minimum equal to maximum is valid; minimum greater than maximum is invalid.
- Tag Any and All modes remain distinct and reject a mode without tags.
- Independently authored scenarios pin logical And, exact strings, missing-value behavior, and inclusive byte endpoints; Rust validates their closed command/result documents without querying an asset repository.
- Strictly ordered asset results, optional missing format/size, and sorted tags round-trip without numeric truncation or undeclared metadata projection; continuation obeys the shared contract.
- Missing and inaccessible root fixtures preserve the validated request root in `root_path`, contain only that field plus their exact `root_not_found|root_access_denied` discriminator, reject unknown or surplus fields, prove enumeration did not begin, and expose neither matches nor a continuation token.
- Every one of the five common budget discriminators has the shared exact charge/check/tie boundary scenarios and yields only closed DiscoveryBudgetExceeded without partial matches/token.

- **Done when:** cargo test -p slingshot-domain --test find_assets_by_metadata validates the shared exact AssetByteLength request/result grammar and domain, bounds, exact root-anchor failures, pagination/continuation, deterministic result shapes, and the complete deployment-era-neutral `dam:Asset`/format/original-size/tag/metadata conformance inventory.
