---
id: model-context-protocol-fuzzing
title: "Model Context Protocol Fuzzing"
workstream: "0035"
kind: task
depends_on:
  - configuration-parser-fuzzing
  - local-protocol-fuzzing
  - agent-stream-fuzzing
gated: false
touches:
  - fuzz/Cargo.toml
  - fuzz/fuzz_targets/model_context_protocol_message.rs
  - "fuzz/corpus/model_context_protocol_message/**"
  - crates/slingshot-development/tests/model_context_protocol_fuzz_corpus.rs
status: planned
merged_as: ""
---
# Model Context Protocol Fuzzing

The standard-input/output server receives untrusted protocol envelopes on the same process that can reach a daemon. This target proves invalid bytes stop at the protocol boundary and output always remains framed.

**Steps:**

1. Commit current discovery/per-request metadata, legacy initialize/initialized, tool and resource listing, tool invocation, exact five-field command identity plus separate canonical-contract artifact/digest/dual-annotation provenance, and one-at-a-time daemon-runtime, author-agent-transport, canonical-contract, limits, argument-schema, and result-schema drift with every other role fixed. Include the exact twelve-row registry-to-`readOnlyHint`/`destructiveHint`/`idempotentHint` mapping; raw canonical-byte and typed-array-order cases; exact `1.0.0+01`/`1.0.0-01` and `AssetByteLength` boundary vectors; canonical/noncanonical command inputs; every revised closed failure; maximum-inline and first-over-inline complete maintenance preview/applied/replayed values plus the daemon-owned operation-free `maintenance_result_access` branch, exact `(AuthorTargetIdentityDigest, MaintenanceResultIdentifier)` association, reviewed-source/content digests, length/media/revision/owner metadata, canonical maintenance-result URI, authenticated target-plus-identifier `MaintenanceResultMetadata` lookup, and closed unreadable refusal before any `MaintenanceResultRead`; both recovery-evidence variants and every legal/illegal result-unavailable pairing; target-qualified resources; progress, cancellation, notifications, batches, duplicate fields, malformed, truncated, deeply nested, and oversized seeds.
2. Register the Model Context Protocol target over both production JavaScript Object Notation Remote Procedure Call revision adapters and session state using an inert daemon dispatcher.
3. Assert rejected input never calls the dispatcher and accepted input emits only complete canonical protocol messages. For both revisions, independently recompute `slingshot.daemon-runtime-contract/1` and `slingshot.author-agent-transport-contract/1` from exact repository/embedded bytes and sidecars; compare the runtime digest with Hello and the transport digest with current executor/receipt/status/result/resource/maintenance provenance; and refuse one-at-a-time mismatch before daemon operation dispatch, schema interpretation, or rendering while retained control remains diagnostic-only. Then enforce the exact five-field command identity, separate canonical-contract provenance, twelve-row annotation mapping, and raw-before-schema-before-typed ordering.
4. Exercise era selection, current result/cache decoration, legacy lifecycle ordering, request-identifier preservation, cancellation idempotency, and unknown-method behavior across generated sequences.
5. Add the deterministic seed runner to ordinary automation.

**Tests:**

- Malformed, batched, duplicate-field, uninitialized, and over-bound inputs never reach daemon dispatch.
- Every output line parses as one complete protocol message with no diagnostic bytes.
- Every accepted current result validates against the pinned 2026-07-28 schema and carries its required modern members; every legacy result validates against the pinned 2025-06-18 schema and omits modern-only members; both reject missing/duplicate recovery evidence and every illegal terminal kind/disposition/certainty combination.
- Leading/trailing Unicode 15.1 White_Space in SearchPhrase, permuted/duplicate asset sets, invalid `AssetByteLength`, token decoding/rewriting, stale or role-swapped provenance, a differently mapped safety annotation, an unknown/aliased/cross-command failure, and any missing/surplus reason/budget/path/index field are rejected before dispatcher access. Accepted phrase bytes, ascending UTF-8 asset arrays, and zero/maximum bounded asset lengths survive both revisions exactly.
- Every maximum registered semantic failure fits the inline `operation_terminal_error` branch. Maximum inline command successes and the next canonical command byte choose the same daemon-owned `structured_result_artifact_access` outcome for the operation `structured_result` slot as CLI; `structuredContent`, canonical text content, and result-resource bytes preserve the fake-agent semantic object and digest without aliases or loss.
- Runtime-only and transport-only drift cases keep every command identity byte fixed and fail independently in both protocol revisions before an operation-capable dispatcher or result renderer observes the request; a retained-control diagnostic cannot be misreported as compatible execution.
- The largest inline maintenance value and first over-inline complete preview/receipt choose exactly the CLI branch. Over-inline bytes remain canonical and out of band through `maintenance_result_access`, never an operation `structured_result` slot; the exact operation-free identifier derivation, association metadata/owner, canonical URI, authenticated target-and-identifier-only `MaintenanceResultMetadata` response fields, expected-digest handoff, `MaintenanceResultRead` same-handle stream, and reviewed-source/content digests match across both Model Context Protocol revisions. No decoder inverts the identifier hash, and an unreadable association returns the closed refusal before any partial metadata or content. Lookup/read accepts only unchanged identity-bound metadata or the sole current-preview-to-application-receipt owner/revision transition before read start; every other transition refuses. Apply transfer and exact replay preserve identity/bytes, while supersession and completed-receipt retirement make only the contract-selected associations unreadable, and no corpus assumes the complete maintenance value fits below 4096 bytes.
- Request identifiers survive all valid request/response paths exactly.
- Replayed cancellation and unknown notifications leave the following valid request serviceable.

- **Done when:** `cargo test -p slingshot-development --test model_context_protocol_fuzz_corpus` and `scripts/run_fuzz_target model_context_protocol_message` prove independent daemon-runtime/author-agent-transport and command-role drift refusal, raw-before-schema-before-typed enforcement, complete inline/operation-free-`maintenance_result_access` parity and lifecycle, exact version/failure/recovery schemas, and canonical standard output for every retained input, and `scripts/quality` succeeds.
