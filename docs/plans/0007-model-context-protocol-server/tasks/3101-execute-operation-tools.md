---
id: execute-operation-tools
title: "Execute Operation Tools"
workstream: "0031"
kind: task
depends_on:
  - pin-machine-outcome-size-budget
gated: false
touches:
  - crates/slingshot-command-line/src/model_context_protocol/operation_execution.rs
  - crates/slingshot-command-line/tests/model_context_protocol_operation_execution.rs
status: done
merged_as: "fa7da842babb0d10c47e8af428e3e5e52c41a6e0"
---
# Execute Operation Tools

Validate tool arguments, connect to the current target/revision daemon, and return detached receipts or truthfully classified attached terminal outcomes across local reconnects.

**Steps:**

1. Commit tool-call transcripts for the existing complete operation/control surface plus exact daemon-runtime/author-agent-transport/canonical-contract/five-field provenance acceptance and independent drift refusal, the five-required/seven-optional operation-key matrix, supplied-key preservation, omitted-optional-key one-time generation and reconnect/retry reuse, byte-preserved/rejected SearchPhrase, canonical/permuted/duplicate FindAssets sets, exact-bound opaque continuation, inline/externalized complete maintenance with the operation-free target-qualified maintenance-result identifier/reference, and every revised continuation/configuration/FileVault/add-component failure with its exact closed fields and Plan 0005 disposition.
2. Before dispatch, recompute exact `DaemonRuntimeContractDigest` and `AuthorAgentTransportContractDigest`; authenticate the canonical-contract artifact/manifest/annotations and exact wire name/`1.0.0`/limits/role-schema provenance; validate exact raw command bytes under `slingshot.command-canonical-json/1`, ordinary Draft 2020-12 decoded shape, and typed/cross-field construction in that order. Convert a supplied operation key losslessly. For an intrinsically idempotent command whose optional key is absent, invoke the injected identifier generator exactly once after validation and before the first local request, store the generated identifier in the active tool-request state, and reuse it for every daemon reconnect and retry until that request completes or detaches; never regenerate it and never derive it from the JSON-RPC request identifier. Then use Plan 0006 daemon/submission/observation components; reject request or runtime/transport/schema drift before versioned daemon dispatch.
3. Keep attached execution subscribed until terminal, cancellation, or end-of-input; reconnect and resubscribe from the last durable revision after local daemon loss without emitting a synthetic operation failure.
4. For command tools, return `isError: true` on terminal failure while preserving the exact semantic failure object and conditional authority payload. For observation tools, return `isError: false` when successfully reporting the same failure. Reject an alias, missing/surplus field, or failure outside the exact command/version schema; neither `isError` nor failure spelling establishes authority.
5. Implement current-target-only `operation_resume` with required expected revision and exact expected recovery category through Plan 0006/Plan 0004. Return truthful current status with applied or replayed durable receipt; prove exact committed sources schedule nothing but replay after later/terminal/restart state, while stale-revision, changed-category, active, missing, historical, or receipt-bound-exhausted fresh attempts schedule nothing and return control errors.
6. Preserve complete maintenance preview/applied/replayed bytes inline only through the machine budget and otherwise require Plan 0004's daemon-owned operation-free `MaintenanceResultAssociation` with the exact target-qualified Model Context Protocol reference, identifier, kind, reviewed source digest, content digest, length, media, revision, and owner identical to CLI JSON. Never route maintenance through an operation `structured_result` slot or invent an operation identity. Assert no tool branch opens an Adobe Experience Manager connection or constructs a publisher target.

**Tests:**

- `model_context_protocol_operation_execution` replays all tool-call transcripts and exact envelopes, including all five missing-required-key refusals, all seven omitted-optional-key successes, supplied-key preservation, exactly one generated identifier across multi-reconnect/multi-retry attached and detached request paths, detach receipts, both recovery-evidence variants, durable recovery-resume apply/replay/category conflict, inline/externalized complete maintenance controls, contextual observation error flags, every legal conditional terminal payload including result unavailable, local failure, transparent exact-provenance daemon replacement, and repeated supplied keys across separate protocol processes resolving to one durable operation.
- Network recorders prove the server process communicates with the local daemon only.
- Runtime mismatch makes zero versioned service calls while retained control remains usable; transport/schema/canonicality failures make zero execution calls. Revised failure and operation-free target-qualified maintenance reference transcripts are byte-identical to Plan 0006 JSON and lose no reason/budget/path/count fact, maintenance identifier, association fact, or content digest.

- **Done when:** `cargo test -p slingshot-command-line --test model_context_protocol_operation_execution` proves exact runtime/transport/canonical-schema provenance and ordered canonical input validation, classification-derived operation-key requirements with one request-owned generated identifier, lossless revised failures plus inline/externalized maintenance CLI parity, distinct execution/observation `isError`, durable recovery replay, truthful authority, and detach-only cancellation.
