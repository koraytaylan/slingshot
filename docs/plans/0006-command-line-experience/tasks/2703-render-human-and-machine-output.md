---
id: render-human-and-machine-output
title: "Render Human And Machine Output"
workstream: "0027"
kind: task
depends_on:
  - observe-operation-state
gated: false
touches:
  - crates/slingshot-command-line/src/artifact_access.rs
  - crates/slingshot-command-line/src/human_renderer.rs
  - crates/slingshot-command-line/src/machine_outcome_envelope.rs
  - crates/slingshot-command-line/src/machine_readable_renderer.rs
  - crates/slingshot-command-line/src/progress_renderer.rs
  - crates/slingshot-command-line/tests/output_rendering.rs
  - "crates/slingshot-command-line/tests/fixtures/machine-outcome-envelope/**"
status: done
merged_as: ""
---
# Render Human And Machine Output

Render concise human outcomes or one canonical versioned `MachineOutcomeEnvelope` while keeping every progress update off standard output.

**Steps:**

1. Commit golden values for the complete existing envelope inventory plus the operation-free `maintenance_result_access` branch, every revised Plan 0003 semantic failure, and the four closed interruption local-error variants. Cover the five continuation literals; all four configuration lookup outcomes and lookup budgets, unsupported/malformed reasons, value budgets, and result budgets; every FileVault pattern/profile/filter/anchor/construction/cleanup/publication outcome and eight evaluation budgets; add-component's eight authoritative-no-effect failures including `parent_not_orderable` plus `mutation_outcome_unknown`; and exact pre-receipt, post-receipt observation, local operation-artifact-transfer, and local maintenance-result-transfer interruption fields. Include every permitted exact field and reject aliases, missing/surplus fields, and cross-command failure objects.
2. Define the versioned `MachineOutcomeEnvelope` shared by command-line JSON and Plan 0007 as the closed fourteen-tag union in architecture. Make `operation_terminal_error` retain the exact schema-validated semantic failure object alongside Plan 0004's conditional authority evidence; never replace the category with prose, infer a field, or drop its reason/budget/path/count. Preserve the existing recovery and terminal disposition invariants. Define closed `local_application_error` interruption variants exactly as architecture: pre-receipt carries only admission unknown plus a retry identifier and no durable operation claim; post-receipt observation carries the validated accepted/replayed admission and revision beside the durable operation identifier; artifact transfer carries only durable operation/artifact identifiers and no local path; maintenance-result transfer carries only target digest/maintenance-result identifier and no synthetic operation or local path. Keep every control/local error structurally unable to claim terminal evidence or a remote semantic failure.
3. Define `command_artifact_access` by projecting the schema-validated command-specific artifact result and replacing each `ArtifactDescriptor` leaf with an access entry containing that complete descriptor, the same target digest, and a canonical percent-encoded `slingshot://profiles/{profile}/environments/{environment}/targets/{author_target_identity_digest}/operations/{operation_identifier}/artifacts/{artifact_identifier}` URI; preserve every other result member exactly and reject filesystem paths and noncanonical identifiers. Keep `structured_result_artifact_access` as the daemon-created canonical `application/json` operation-artifact entry for an over-inline logical command. Define `maintenance_result_access` as the exact closed Plan-0004 association projection with target, identifier, kind, reviewed source digest, content digest, length, fixed media type, association revision, retention-owner class, and canonical URI `slingshot://profiles/{profile}/environments/{environment}/targets/{author_target_identity_digest}/maintenance/results/{maintenance_result_identifier}`; it has no operation/slot/path field.
4. Bound every inline dynamic name, error, interruption identifier, descriptor, URI, maintenance fact, and nested value with named constants; set `MAXIMUM_MACHINE_OUTCOME_ENVELOPE_BYTES` strictly below the pinned FSM 4096-byte canonical structured acknowledgement cap. Preserve the daemon/local contract's separate operation-artifact and operation-free maintenance-result dispositions and reject an over-bound inline value as an invariant violation, while making no claim that a complete maximum maintenance manifest or receipt itself fits below 4096 bytes.
5. Implement separate human, machine-readable, and progress renderers with canonical field order and explicit stream ownership. Machine rendering embeds the semantic failure object byte-for-byte and writes exactly one interruption envelope when selected. Human interruption stdout is empty and stderr uses only the three exact architecture templates; ordinary human rendering prints the exact failure literal and every registered field without exposing unavailable redacted/configuration/filter/package data.
6. Exercise terminal and redirected modes and compare standard output and standard error independently byte for byte.

**Tests:**

- `output_rendering` pins both streams for every golden value and output mode.
- JSON cases parse as exactly one `MachineOutcomeEnvelope`, round-trip to identical canonical bytes, select exactly one advertised tag for every command leaf, preserve every legal conditional daemon recovery/terminal payload and every non-artifact command-result member exactly, retain the load Artifact requested path, reject missing, duplicate, or cross-branch recovery evidence and every invented/missing/cross-kind terminal certainty, prevent control or local errors from claiming terminal authority, and contain no local path in artifact access entries.
- Receipt, detached submission, status/recovery/resume, result, artifact, list page, maintenance, configuration, daemon control, all error fixtures, and all four phase-specific interruption variants pin the complete outcome inventory.
- Each revised continuation/configuration/FileVault/add-component failure round-trips through machine output without alias or field loss and through human output with the same literal/registered facts; unknown version/category/reason/budget, `parent_not_orderable` omission, and any leaked configuration key/value or filter expression are rejected.
- Interruption fixtures reject a durable-operation field/state/revision on pre-receipt admission unknown, missing receipt facts post-receipt, any destination/staging path on either transfer interruption, any operation/slot on maintenance-result interruption, terminal authority on all four, and any output besides one JSON envelope or the exact human stderr template.
- Boundary cases immediately below, at, and above every branch and complete-envelope limit prove an over-bound inline command or maintenance value is rejected and the corresponding daemon outcome supplies the canonical JSON operation-artifact or operation-free maintenance-result access entry. Maximum interruption, recovery-evidence, result-unavailable, largest inline maintenance, and maintenance-reference branches remain strictly below 4096 bytes; complete over-inline maintenance bytes remain intact out of band with the same digest.

- **Done when:** `cargo test -p slingshot-command-line --test output_rendering` proves every CLI JSON outcome selects one canonical inline, operation-artifact, or operation-free maintenance-result branch with the exact URI, complete maintenance bytes/digests are never truncated, all revised semantic failures and four phase-specific interruption variants remain exact and lossless in machine/human rendering, conditional authority remains legal, bounds fit, artifacts expose no path, and progress remains on stderr.
