---
id: derive-operation-schemas
title: "Derive Operation Schemas"
workstream: "0030"
kind: task
depends_on:
  - derive-operation-tools
gated: false
touches:
  - crates/slingshot-command-line/src/model_context_protocol/schema_projection.rs
  - crates/slingshot-command-line/tests/model_context_protocol_schemas.rs
  - crates/slingshot-test-support/fixtures/model-context-protocol/schemas/**
status: planned
merged_as: ""
---
# Derive Operation Schemas

Project every registry and control argument into a strict canonical input schema and each tool's applicable outcomes into its shared machine-envelope output schema.

**Steps:**

1. Commit projection-provenance and schema goldens for all twelve command tools. Each record carries exact canonical `schemas/command-canonical-json-1.json` bytes/digest/format, the command-schema-manifest contract digest, both role roots' exact `x-slingshot-canonical-json-contract-sha256` annotations, wire name, exact `1.0.0`, limits digest, argument/result schema roles/digests, source `$id`, generated input/output digest, all manifest bounds, and the existing control/envelope branches. Include every revised closed continuation/configuration/FileVault/add-component failure object; do not add the contract digest to `SelectedCommandContractIdentity`.
2. Recompute and authenticate the exact Plan 0003 canonical-contract digest, manifest value, role annotations, and annotated role-schema digests. Compose rather than recreate each command argument schema with bounded `operation_key` required exactly for `NotIntrinsicallyIdempotent` and optional exactly for `IntrinsicallyIdempotent`, plus attached/detached execution, retain the exact contract annotation, and deterministically project the owning argument role's ordered-array pointer inventory. Preserve closed objects, canonical key order, all manifest-derived constraints, and fail hard on an unsupported type or provenance mismatch. Keep the existing strict control-tool schemas.
3. Retain the exact raw `params.arguments` byte slice. After the containing request passes its applicable official protocol-definition oracle, validate projected command bytes and ordered arrays under the language-neutral `slingshot.command-canonical-json/1` validator first, the ordinary Draft 2020-12 decoded shape second, and the typed/cross-field constructor third; never parse and reserialize into compliance. SearchPhrase is byte-preserved and never trimmed/normalized; FindAssets media-format/tag arrays must be already strictly ascending UTF-8 and unique, and each optional byte-range endpoint must be canonical `AssetByteLength` in `0..=9_223_372_036_854_775_807` before range ordering is checked; command continuation is opaque/nonempty/control-free through the exact bound and exclusive with Initial fields; ordered predicates/package filters retain order.
4. Implement each tool `outputSchema` as the MachineOutcomeEnvelope restricted to its applicable exact result schema, artifact projection, semantic failures, and existing receipt/status/control/local branches. Preserve only registered failure literals and their exact reason/budget/path/count fields; reject aliases, arbitrary agent codes, cross-command/cross-version failures, and unconstrained error objects. Retain the existing conditional recovery/terminal authority union.
5. Validate every example through the raw-byte validator, exact source/generated Draft 2020-12 schemas, typed constructor, and CLI oracle in that order. Record the intentional schema-versus-stronger-constructor distinction where JSON Schema cannot express raw serialization or UTF-8 lexical ordering; canonical accepted documents and all result/failure shapes must agree exactly. CLI flag permutations are sorted by its constructor before canonical serialization, while an already serialized noncanonical Model Context Protocol document is rejected before ordinary schema success can substitute.

**Tests:**

- `model_context_protocol_schemas` byte-compares every generated input and envelope-wrapped output schema, accepts an omitted key for exactly the seven intrinsically idempotent commands, rejects a missing key for exactly the five non-intrinsically-idempotent commands including read-only content load, and rejects an empty or over-bound supplied key for every command tool; it also rejects missing/invalid expected revisions, historical resume targets, and any undeclared wait-timeout field.
- Negative oracles prove a bare command result, a command-artifact projection missing the load requested path, missing/duplicate/cross-branch recovery evidence, an illegal terminal kind/certainty/disposition combination, an outcome tag impossible for that tool, and each CLI-signal-only interruption local-error variant do not satisfy any tool output schema; detached receipts and each complete control-result tag satisfy their owning schemas.
- Differential cases prove source/generated schema parity and record whether rejection occurred at raw bytes, ordinary schema, or typed semantics. They cover member order/duplicate/escape/integer and typed-array order vectors; untrimmed phrases; canonical, permuted, and duplicate asset sets; `AssetByteLength` zero/maximum/next/negative/fraction/exponent/nonminimal/overflow and valid-inverted-range cases; below/at/above opaque token bounds; every revised exact failure reason/budget; stale version/limits/role digests; and contract-only drift with fixed manifest-independent five-field identity and fixed role bytes/digests.
- Contract provenance cases independently mutate the contract artifact, manifest digest, each root annotation, and each role byte/digest. No case reaches typed conversion, tool execution, or resource serving, and a standard-schema pass cannot excuse a raw-byte failure.

- **Done when:** `cargo test -p slingshot-command-line --test model_context_protocol_schemas` proves authenticated canonical-contract/annotation/five-field provenance, exact classification-derived five-required/seven-optional operation-key schemas, raw-byte then Draft 2020-12 then typed ordering, independent drift refusal, canonical request parity, and command-specific closed result/failure envelope schemas alongside the existing control/recovery contracts.
