---
id: resource-mutation
title: "Resource Mutation Results and Inline Binary"
workstream: "0040"
kind: task
depends_on:
  - operational-contract-limits
gated: false
touches:
  - crates/slingshot-domain/src/command/resource_mutation.rs
  - crates/slingshot-domain/src/command/mod.rs
  - crates/slingshot-domain/Cargo.toml
  - policy/workspace-capabilities.toml
  - crates/slingshot-domain/tests/resource_mutation.rs
status: planned
merged_as: ""
---
# Resource Mutation Results and Inline Binary

Sixteen writes have the same whole answer: the address they changed. Saying that sixteen times produces sixteen chances to say it differently. This task lands the shared mutation results, the reference policy every destructive content command states, and the one bounded inline payload that carries bytes inward.

**Steps:**

1. Implement `ResourceMutationResult` carrying one validated repository path, with the request-context rule that the path it reports is the one the request determined.
2. Implement `DeletedResourceResult` carrying the removed address and a removed-node count bounded by `MAXIMUM_DELETED_NODES`, and `MovedResourceResult` carrying source, destination, and an adjusted-reference count bounded by `MAXIMUM_ADJUSTED_REFERENCES`.
3. Implement `ReferencePolicy` as the closed `RefuseWhenReferenced` or `IgnoreReferences`, with no default, so a caller states it.
4. Implement `InlineBinaryPayload` as a bounded media type and standard Base64 with mandatory padding: refuse an encoded length over `MAXIMUM_INLINE_BINARY_ENCODED_BYTES` before decoding, refuse a decoded length over `MAXIMUM_INLINE_BINARY_DECODED_BYTES` after it, refuse a character outside the standard alphabet, refuse missing or excess padding, and refuse an interior line break.
5. Decode through the workspace's existing Base64 capability by adding `slingshot-domain` to that capability's owners rather than writing a second decoder.

**Tests:**

- Each result round-trips byte-identically, rejects unknown fields, and rejects a null member.
- Both counts are accepted at their exact limit and refused one past it.
- The reference policy round-trips as `refuse_when_referenced` and `ignore_references` and has no default, so an absent member is a refusal rather than a silent choice.
- The payload accepts the exact encoded and decoded bounds and refuses each by one byte, refuses a non-alphabet character, refuses absent and doubled padding, refuses an embedded line feed, and decodes to exactly the bytes it was given for a fixed fixture.
- The capability row lists the new owner and the dependency policy accepts the manifest edge.

- **Done when:** `cargo test -p slingshot-domain --test resource_mutation` and the dependency policy both pass, with every bound proved at the limit and one step beyond it.
