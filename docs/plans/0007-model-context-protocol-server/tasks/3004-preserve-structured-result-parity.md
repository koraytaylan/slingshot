---
id: preserve-structured-result-parity
title: "Preserve Structured Result Parity"
workstream: "0030"
kind: task
depends_on:
  - derive-operation-schemas
  - expose-operation-resources
gated: false
touches:
  - crates/slingshot-command-line/src/model_context_protocol/result_projection.rs
  - crates/slingshot-command-line/tests/model_context_protocol_results.rs
status: done
merged_as: "409644e37ddcd0733a20c1d6850ec932dcd4a131"
---
# Preserve Structured Result Parity

Map the complete receipt/status/result/artifact/list/maintenance/error inventory into the one command-line-owned `MachineOutcomeEnvelope` used byte-semantically by structured and text content.

**Steps:**

1. Commit shared daemon-outcome fixtures and expected CLI JSON, structured content, text, contextual error flag, and resource links for the existing full outcome inventory plus every revised continuation/configuration/FileVault/add-component semantic failure literal and registered field. Bind every command result fixture to authenticated daemon-runtime/author-agent-transport/canonical-contract/annotation/five-field provenance and its raw-byte/schema/typed validation-stage result. Include complete maintenance preview and applied/replayed bytes immediately below/at/above inline externalization plus their exact Plan 0004 `MaintenanceResultIdentifier`, target-qualified URI, association kind/source/content/length/media/revision/owner, current/transfer/replay/supersede/retire lifecycle, and fresh-process metadata-then-read retrieval with unchanged or checked exact-apply-transfer Start.
2. Reuse Plan 0006's exact fourteen-tag envelope and preserve the raw-canonical, schema-validated, typed Plan 0003 semantic failure object byte-for-byte on terminal errors. Reject aliases, missing/surplus facts, unknown reason/budget literals, cross-command failures, contract/annotation drift, fields that would expose redacted configuration/filter/package data, and the four CLI-signal-only interruption local-error variants that Model Context Protocol cancellation must suppress rather than render. Preserve the existing conditional recovery/terminal authority union without deriving authority from the failure literal or `isError`.
3. Project command-artifact results by replacing only their `ArtifactDescriptor` leaves with access entries containing the descriptor plus canonically percent-encoded target-qualified URI and no filesystem path; preserve every non-artifact logical-result member exactly. Keep daemon-created operation `structured_result` access as the single entry for an over-inline command value. For over-inline maintenance, project only Plan 0004's operation-free target-qualified `maintenance/results/{maintenance_result_identifier}` access entry, preserving selected target, kind, exact reviewed preview digest, exact preview or applied/replayed receipt-byte digest, length, media, association revision, and retention owner without inventing an operation identity. Duplicate an access URI in tool content resource links only as an affordance.
4. Implement one semantic projection, keeping malformed protocol requests as JSON-RPC errors and all well-formed tool outcomes as tool results. Set `isError: true` for an attached command's terminal failure and for a failed control/local action; keep successful status/wait/result/list/artifact/maintenance observations `isError: false` even when they report a failed or fail-closed operation.
5. Parse every text content value and compare its canonical bytes with `structuredContent` serialization and command-line JSON bytes.

**Tests:**

- `model_context_protocol_results` covers every Model Context Protocol-applicable tag, accepted/replayed detach, recovery/resume status, artifact combinations, list/maintenance controls including `maintenance_result_access`, all terminal dispositions, operation-control/local errors, and every per-tool `isError` rule; it proves configuration-check, daemon-control, and all four CLI-signal-only interruption variants belong to the shared CLI union but to no Model Context Protocol tool schema.
- Parity assertions compare the same fixture through command-line and Model Context Protocol renderers and prove the load Artifact requested path survives byte-identically.
- Parity assertions cover every revised failure and prove exact literal, reason/budget/path/count fields, disposition, and canonical bytes equal CLI JSON and parsed text; complete over-inline maintenance produces the identical CLI/structured/text operation-free maintenance-result reference, identifier, and content digest without truncation, and a fresh-process resource read obtains that otherwise non-invertible digest from exact target-and-identifier metadata, authenticates unchanged or checked exact-apply-transfer Start, and returns the exact canonical document through replay until supersession or retirement; no cache, hash inversion, or protocol decoration changes semantic content.
- Authority assertions prove each legal conditional daemon recovery and terminal payload survives byte-for-byte, no certainty is invented for either authoritative remote branch, `ResultUnavailable` and terminal authoritative remote success are mutually required, control/local errors cannot carry terminal facts, observation of failure stays a successful tool call, and `isError` alone never selects a disposition.

- **Done when:** `cargo test -p slingshot-command-line --test model_context_protocol_results` proves runtime/transport/canonical-schema authenticated and raw-byte/schema/typed-validated results remain byte-identical across CLI/structured/text success, complete inline or operation-free target-qualified externalized maintenance with metadata-authenticated lifecycle-valid reads in a fresh process, and revised failure envelopes with no aliases/lost fields, while recovery, observation, and authority semantics remain exact.
