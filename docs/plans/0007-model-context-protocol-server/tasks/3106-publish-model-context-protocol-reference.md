---
id: publish-model-context-protocol-reference
title: "Publish Model Context Protocol Reference"
workstream: "0031"
kind: chore
depends_on:
  - prove-server-process-boundaries
gated: false
touches:
  - crates/slingshot-command-line/tests/model_context_protocol_reference.rs
  - docs/MODEL_CONTEXT_PROTOCOL.md
status: done
merged_as: ""
---
# Publish Model Context Protocol Reference

Document launch, dual-revision decoration, generated tools and target-qualified resources, machine-envelope parity, terminal attached waits, reconnect, progress, cancellation detachment, and output-pressure fail-stop rules from tested metadata.

**Steps:**

1. Commit a documentation coverage fixture containing the exact `["2026-07-28","2025-06-18"]` authority; modern discovery/capability/method/error inventory; legacy exact-version echo and schema-valid-unsupported-version successful fallback; both complete official protocol-schema provenance records and their inbound-request/notification plus outbound-response validation rules; exact daemon-runtime and author-agent-transport contracts; every command's canonical-contract/annotation and exact five-field `1.0.0`/limits/role-schema origin; the classification-derived five-required/seven-optional operation-key matrix, caller-key preservation, and once-generated request identifier lifetime across reconnect/retry; raw-byte/schema/typed order; canonical phrase/asset/token rules; every revised closed semantic failure and fields; per-tool/resource schema including full five-field durable operation-result comparison and the exact operation-free target-qualified maintenance template, target-and-identifier metadata lookup, authenticated read Start/stream, sole checked apply-transfer race, and lifecycle; inline/externalized complete maintenance parity; CLI parity; explicit exclusion of CLI-signal-only local errors; and the existing outcome, recovery, resource, size, execution, queue, diagnostic, fail-stop, detachment, and retained-instance cleanup contracts.
2. Render registry-derived provenance, input, result, and failure tables without aliases, and add concise current and real-FSM-compatible direct-call transcripts to `docs/MODEL_CONTEXT_PROTOCOL.md`.
3. Add link, generated-section, example-parse, claim-coverage, and present-state-language tests tied to the golden transcript corpus.

**Tests:**

- `model_context_protocol_reference` compares generated catalog/resource sections and validates both example transcripts.
- Link and claim checks pin the two complete official protocol schemas and both-direction conformance; the exact ordered modern discovery/error values and correct legacy fallback negotiation; modern/legacy member differences; exact runtime/transport/canonical-contract/annotation/five-field tool/operation-result provenance; the exact required/optional operation-key matrix and one generated identifier per omitted-key active request across reconnect/retry; command-execution versus observation `isError` semantics; conditional recovery/result-unavailable evidence; current-target expected-revision/category recovery apply and durable replay; inline/externalized complete maintenance controls plus a fresh-process target-qualified metadata-then-read through unchanged or sole checked apply-transfer Start, replay, supersession, and retirement without cache, hash inversion, or a maximum-manifest-under-4096 claim; active-request duplicate/saturation/release; artifact metadata-only resources; reconnect; cancellation/end-of-input detachment without a CLI interruption envelope; drop-capable stderr; retained-instance cleanup; and slow/closed/full-output bounded exit.
- A text-policy case rejects TODO markers and past/future implementation narration while excluding `docs/plans` from the product-document scan.

- **Done when:** `cargo test -p slingshot-command-line --test model_context_protocol_reference` proves the reference matches both revisions' exact ordered support/negotiation/discovery/error behavior, runtime/transport/command provenance, classification-derived operation-key policy and request-owned generated-identifier lifetime, ordered canonical inputs, inline or operation-free target-qualified externalized maintenance and metadata-authenticated read lifecycle, closed lossless result/failure schemas, CLI parity, and recovery/resource/size/process contracts.
