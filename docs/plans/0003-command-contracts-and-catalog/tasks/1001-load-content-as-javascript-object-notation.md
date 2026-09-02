---
id: load-content-as-javascript-object-notation
title: "Load Content as JSON"
workstream: "0010"
kind: task
depends_on:
  - artifact-descriptors
  - command-module-scaffold
  - property-values
  - repository-path
gated: false
touches:
  - crates/slingshot-domain/src/command/load_content_as_javascript_object_notation.rs
  - crates/slingshot-domain/tests/fixtures/commands/load_content_as_json/**
  - crates/slingshot-domain/tests/load_content_as_javascript_object_notation.rs
status: done
merged_as: "8e47840165ca45a2845dbab3617f6ee85e139009"
---
# Load Content as JSON

Represent loading one repository subtree as JSON with an explicit path and bounded depth.

**Steps:**

1. Commit canonical requests/results and language-neutral agent conformance scenarios for omitted/default/zero/maximum/over-maximum depth; tree/SNS/truncation order; every JCR type/cardinality; minimal Long, scale-preserving Decimal, canonical millisecond Date plus years/precision outside the supported subset, absolute/relative Path and Name lexical subsets, bounded exact Reference/WeakReference spelling, RFC 3986 absolute URI accepted/rejected/percent-spelling boundaries, empty multi-value; minimal nonnegative signed-64-bit Binary length plus negative/overflow rejection; Double ordinary/positive-zero/negative-zero/infinities/distinct NaN payload bits; exact not-found/access-denied/unsupported-value/load-budget failures; canonical loaded-document lengths one below/at/one above 262,144 bytes with outer-envelope bytes varied independently; Inline/Artifact parity; every count/byte/duration bound; pre/post-call late return; and cooperative cancellation before implementation.
2. Implement LoadContentAsJavaScriptObjectNotationCommand with RepositoryPath and optional validated depth: omission becomes DEFAULT_LOAD_DEPTH, zero includes root only, and MAXIMUM_LOAD_DEPTH is inclusive.
3. Implement the exact RepositoryJavaScriptObjectNotationResource tree and RepositoryJavaScriptObjectNotationPropertyValue JCR mapping in the architecture, including explicit `property_type`/`cardinality`, exact supported numeric/date/reference forms with UnsupportedRepositoryValue outside the interoperable subset, metadata-only Binary, sorted direct properties/children, paths on every resource, and `children_truncated`.
4. Implement exactly-one closed Inline or Artifact result from the complete canonical UTF-8 bytes of RepositoryJavaScriptObjectNotationResource alone, excluding outer result/path/descriptor/transport bytes from the threshold charge. A document at or below `MAXIMUM_AGENT_INLINE_LOADED_DOCUMENT_BYTES` must use Inline with path/document. The next document byte through MAXIMUM_LOAD_DOCUMENT_BYTES must use Artifact with path/descriptor constrained to OptionalAlternative `loaded_content_json`, `application/json`, exact `loaded-content.json` suggested file name, and an exact descriptor length equal to the charged document length and at or below its named maximum; neither both nor neither is valid.
5. Declare the stable `loaded_content_json` artifact slot and maximum length in registry/schema metadata while keeping remote location out of the domain result. Its bytes must be byte-for-byte the same schema-valid canonical document charged above; a generic agent response or local presentation bound cannot change the exact 262,144-byte loaded-document boundary.
6. Implement exact `not_found`, `access_denied`, `unsupported_repository_value`, and `load_budget_exceeded` structured failure values. The budget failure accepts only `resource_nodes|property_values|property_bytes|serialized_document_bytes|traversal_duration` and contains no partial document or artifact.
7. Pin the external-agent execution contract in the language-neutral scenarios: at cooperative boundaries immediately before and after every repository call and canonical-output step, check cancellation, injected monotonic time, and the next checked count/byte charge in that order. Allow an exact count/byte maximum; treat a call returned at/after expiry as `load_budget_exceeded`/`traversal_duration` before interpreting its payload/error; cancellation stops later calls and publishes no result/artifact. Rust validates exact fake-call traces without claiming to traverse, interrupt, or hard-bound one blocking repository call.
8. Supply request-context validation that requires every success or failure path field and artifact suggested name/slot to match the exact originating LoadContentAsJavaScriptObjectNotationCommand before the result can be persisted or forwarded.

**Tests:**

- Omitted/default, zero, maximum, and over-maximum depth cases assert exact included levels and truncation flags.
- Root and nested content paths reuse RepositoryPath behavior.
- Single/multiple JCR type fixtures round-trip losslessly; Binary never carries bytes and uses exact minimal `0|[1-9][0-9]*` length through the signed 64-bit maximum, while Double is exactly 16 lowercase hexadecimal binary64 bits preserving signed zero and retrieved nonfinite payloads without JSON numbers.
- Direct properties and child resources have exact deterministic order; empty repository multi-values retain type/cardinality; same-name siblings use the pinned tie break.
- Language-neutral scenarios require missing, denied, and unsupported roots/subtrees, including a Date/Name/Path outside Slingshot's interoperable subset, to fail distinctly with no partial document; Rust validates their closed canonical input/result inventory without claiming repository execution.
- URI fixtures accept only exact bounded RFC 3986 `URI` ABNF with required scheme/ASCII/valid percent triplets and preserve spelling without normalization; non-ASCII literal, malformed escape, relative, and over-bound values fail the entire load as unsupported.
- Values immediately below, at, and above every resource/property/string/binary-metadata/serialized-byte/duration bound prove bounded allocation. A complete canonical loaded document at 262,144 bytes is Inline and the next document byte selects Artifact; changing only echoed path or outer envelope size cannot move that boundary. Crossing a logical/document budget yields `load_budget_exceeded` with no partial artifact.
- Late-return and cancellation fixtures prove the exact last cooperative boundary, discard the accumulated document/artifact, and make no claim that injected time or cancellation preempts a fake call before it returns.
- Unknown request and result fields are rejected.
- Artifact results require stable ArtifactIdentifier metadata, the declared `loaded_content_json` slot, exact `application/json` media type, and exact `loaded-content.json` suggested file name and reject inline document or remote location fields.
- No request fixture contains an endpoint, tier, profile, environment, or credential.
- A structurally valid Inline, Artifact, or path-bearing failure copied from a different load request is rejected by request-context validation before artifact acceptance or persistence.

- **Done when:** cargo test -p slingshot-domain --test load_content_as_javascript_object_notation validates exact bounded command/result shapes plus the complete independently authored 6.5/Cloud-neutral depth/tree/order/JCR-value/count-byte/cooperative-duration/late-return/cancellation/error conformance inventory, exact canonical-loaded-document Inline/Artifact boundary, and artifact byte parity without a blocking-call preemption claim.
