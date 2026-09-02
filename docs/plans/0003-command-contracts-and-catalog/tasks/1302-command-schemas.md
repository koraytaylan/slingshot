---
id: command-schemas
title: "Command Schemas"
workstream: "0013"
kind: task
depends_on:
  - load-content-as-javascript-object-notation
  - inspect-open-service-gateway-initiative-configuration
  - query-paths
  - find-pages-containing-phrase
  - find-pages-by-template
  - find-pages-using-components
  - find-assets-by-metadata
  - find-assets-referenced-by-page
  - replicate-content
  - download-content-package
  - create-page
  - add-component
gated: false
touches:
  - crates/slingshot-domain/src/command/schema.rs
  - crates/slingshot-domain/src/command/canonical_json.rs
  - schemas/commands/**
  - schemas/command-canonical-json-1.json
  - schemas/command-canonical-json-vectors.json
  - schemas/command-schema-digest-vectors.json
  - crates/slingshot-domain/tests/command_schemas.rs
  - crates/slingshot-domain/src/command/schema.rs
status: done
merged_as: "c11de60d75f9dc296f44c89a3c077c9524afa414"
---
# Command Schemas

Generate committed language-neutral argument and result schemas from the completed command-specific types so the final registry consumes fixed reviewed digests.

**Steps:**

1. Require every command-specific module's stable wire name, exact manifest-owned `1.0.0`, and typed manifest-owned limits; generate proposed schema bytes from those contracts into test output before accepting implementation and review them against every canonical command fixture. No producer declares a public limit or version ad hoc.
2. Implement ordinary Draft 2020-12 schemas with exact `$schema`, role/versioned `$id`, closed objects, required fields, manifest-derived schema-expressible types/patterns/ranges/counts/uniqueness, literal discriminators, and document-local alternatives. Insert exact `1.0.0` literally in the final URN segment. Do not claim a standard schema validator observes serialized member order, raw UTF-8/escape/integer spelling, or general lexical order between arbitrary array members.
3. Commit one argument and one result schema per command plus `slingshot.command-schema/1` manifest carrying exact version, exact `slingshot.command-contract-limits/1` SHA-256 digest, exact canonical-JSON-contract SHA-256 digest, and separate SHA-256 digests over exact canonical role bytes as 64 lowercase hexadecimal characters. Every schema root carries the canonical-contract digest in exact annotation `x-slingshot-canonical-json-contract-sha256`; standard validators may ignore it, while its inclusion in schema bytes binds both role digests to the separately executed validator. A limits/version/canonical-contract change regenerates every affected schema/descriptor; because `$id` contains the version, a version change necessarily regenerates both role digests.
4. Commit closed machine-readable `schemas/command-canonical-json-1.json`, format `slingshot.command-canonical-json/1`, defining the architecture's raw UTF-8/no-BOM/single-value/no-whitespace, ascending unique member, exact escape, minimal integer, no-nonintegral-number, separator, and array-order rules. Include one exact per-command-role JSON-Pointer inventory mapping every array to `preserve` or a fully defined strict canonical comparator; an absent pointer means preserve and unknown/duplicate pointers or comparators fail. Implement the raw-byte validator independently from standard schema validation.
5. Author independent language-neutral `schemas/command-canonical-json-vectors.json` plus digest vectors for invalid UTF-8/BOM/leading-trailing whitespace, nested member order/duplication, Unicode/control/escape spellings, preserved arrays, every strict lexical comparator below/at/equal/out-of-order, minimal signed Integer, AssetByteLength zero/signed-64-bit-maximum/next/negative/alternate-token forms in argument and result roles, Decimal scale/trailing-zero and rejected alternate spellings, canonical millisecond Date, `slingshot.command-arguments-canonical/1` window omission, argument/result role, and one-bit mutation; do not generate expected bytes/digests from the Rust implementation under test.
6. Add a regeneration test that writes to an isolated temporary directory and compares every schema, canonical-byte contract/vector, and digest artifact byte with the committed files without rewriting them.
7. Add an inventory test proving one argument/result schema producer, committed file, digest-manifest role, canonical-array-pointer inventory, and fixture directory for each command-specific type; the later registry task proves the final enum/descriptor-as-wire-capability/scenario bijection.

**Tests:**

- Every valid canonical fixture first passes the language-neutral raw-byte/order contract, then its corresponding standard schema, then the typed constructor.
- Every invalid fixture is assigned to and rejected at exactly its earliest canonical-byte, standard-schema, or stronger typed-semantic layer, with the distinction recorded in the test table; a schema-only pass is never reported as canonical-byte proof.
- Regeneration is byte-stable across repeated runs.
- Every independent vector produces its exact 32 digest bytes/64 lowercase hexadecimal spelling, while mutation, role swap, `$schema`, or `$id` change fails comparison.
- Missing, extra, renamed, and duplicate schema producers/files fail the pre-registry inventory test.
- Schema descriptions and titles satisfy the production documentation policy.
- Continuation protected payload/binding/failure precedence and profile-neutral durable-authority ownership, phrase/request-set canonical ordering, exact AssetByteLength request/result grammar, exact repository/JCR and distinct OSGi scalar/carrier/key/redaction grammars, the one-getProperties/complete-key-snapshot/zero-read-redaction external trace, literal lookup/value/result/anchor/load/discovery/replication/package/create/add failure shapes, every manifest count/byte/duration literal, exact canonical-loaded-document Inline boundary, restricted package selection plus exact non-widening FileVault regex/XML/profile/structural serialization facts, orderable-parent requirement, artifact declarations, and agent-conformance inventories appear in the appropriate schema, canonical-byte inventory/vectors, or external conformance artifacts without Rust-only assumptions.
- The five rooted discovery result schemas admit `root_not_found|root_access_denied` only as closed objects with `failure` and `root_path`; referenced-asset discovery admits `page_not_found|page_access_denied|page_invalid` only with `failure` and `page_path`. Schema fixtures reject a missing path field, the other path role, matches, continuation token, or any unknown/surplus field on those failures.
- Canonical Semantic Versioning boundaries prove legal build `+01`, illegal numeric prerelease `-01`, and malformed, noncanonical, over-complete-byte, over-combined-identifier, over-per-numeric-identifier, and URN-unsafe raw strings before `$id` construction.
- Every command is exactly `1.0.0`; changing a manifest version changes both version-bearing `$id` values and consequently both role digests, changing a command-relevant limit changes the limits digest and every affected schema/catalog record, and changing the canonical-byte contract changes its digest annotation and both role digests. Fixtures reject stale limits, canonical contract, schemas, or digests. Changing only one role schema under the same version/limits/canonical contract changes only that role digest.
- Standard schemas validate schema-expressible document-local decoded shape only. Raw serialization and typed lexical array order belong to `slingshot.command-canonical-json/1`; cross-document request/result correlations belong to the domain request-context validator and Plan 0005 submitted-command digest. No schema test claims canonical bytes, exact request provenance, or repository semantic re-execution.

- **Done when:** cargo test -p slingshot-domain --test command_schemas separately passes raw canonical-byte/order validation, ordinary Draft 2020-12 decoded-shape validation, and typed semantic validation; proves exact-`1.0.0`/limits-manifest consumption through independent canonicalization/SHA-256 vectors; regenerates every artifact byte-stably; invalidates changed version/limits/schema digests; and completes the command-specific-type-to-schema-to-canonical-pointer-to-fixture inventory needed by the later registry.
