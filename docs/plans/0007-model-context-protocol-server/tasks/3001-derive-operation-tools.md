---
id: derive-operation-tools
title: "Derive Operation Tools"
workstream: "0030"
kind: task
depends_on:
  - serve-current-stateless-revision
  - serve-legacy-initialized-revision
gated: false
touches:
  - crates/slingshot-command-line/src/model_context_protocol/tool_catalog.rs
  - crates/slingshot-command-line/tests/model_context_protocol_tool_catalog.rs
status: done
merged_as: ""
---
# Derive Operation Tools

Generate one tool per registry operation plus complete list/status/wait/resume/result/artifact and maintenance controls from ordered authoritative metadata and the shared machine-outcome contract.

**Steps:**

1. Commit a registry-to-tool snapshot containing for every command origin its wire name, exact `1.0.0`, canonical command-limits digest, argument/result schema digests, titles/descriptions, the exact Plan 0003 access/destructive/intrinsic-idempotency row, its mechanically mapped `readOnlyHint`/`destructiveHint`/`idempotentHint`, its derived required-or-optional `operation_key` presence, and the existing eight fixed controls before implementing projection. Alongside the unchanged five-field identity, record exact `slingshot.author-agent-transport-contract/1` bytes/sidecar digest, exact `slingshot.command-canonical-json/1` artifact digest, command-schema-manifest value, and both role roots' `x-slingshot-canonical-json-contract-sha256` annotations as authenticated provenance.
2. Implement deterministic generation with explicit collision, missing/extra origin, non-`1.0.0` version, author-agent-transport mismatch, canonical-contract artifact/manifest/annotation mismatch, limits-digest drift, role-digest drift, and unsupported-metadata failures before either protocol revision can serve discovery/tools. Recompute exact contract and annotated role digests rather than trusting recorded strings.
3. Compare operation and tool origin sets exactly and run the same catalog through both revision handlers.

**Tests:**

- `model_context_protocol_tool_catalog` pins complete ordered tool descriptors and byte-matches all twelve registry classification rows to the resulting nine-read/three-write, one-destructive/eleven-nondestructive, seven-idempotent/five-non-idempotent annotation matrix, and seven-optional/five-required operation-key matrix; `load_content_as_json` is read-only/nondestructive but non-idempotent with a required key and `replicate_content` is the sole destructive tool with a required key.
- Coverage fails on a missing, duplicate, orphaned, or differently classified operation tool and on any missing or extra fixed control tool.
- A matching wire name with any stale transport digest, canonical-contract artifact/manifest/annotation, or version/limits/schema digest is incompatible and cannot be hidden by otherwise fixed five-field identity, identical annotations, or schema shape; Plan 0007 contains no fallback identity table.

- **Done when:** `cargo test -p slingshot-command-line --test model_context_protocol_tool_catalog` proves authenticated author-agent-transport and canonical-contract provenance around the exact five-part command identity, exact twelve-row registry-to-Model-Context-Protocol safety annotation and operation-key-presence derivation, generated tool equality, independent drift refusal, and the exact eight-control surface.
