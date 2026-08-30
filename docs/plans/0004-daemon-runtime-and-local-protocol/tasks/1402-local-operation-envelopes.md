---
id: local-operation-envelopes
title: "Local Operation Envelopes"
workstream: "0014"
kind: task
depends_on:
  - bounded-local-frame-codec
  - stable-local-control-protocol
gated: false
touches:
  - crates/slingshot-local-protocol/src/message.rs
  - crates/slingshot-local-protocol/tests/message.rs
  - "crates/slingshot-local-protocol/tests/fixtures/messages/**"
status: done
merged_as: ""
---
# Local Operation Envelopes

This task freezes the versioned operation-message vocabulary independently of the retained control protocol, so clients and the server share one typed contract without making daemon inspection depend on execution compatibility.

**Steps:**

1. Author request, response, progress, terminal, malformed, normalized-author mismatch, opaque-authentication-principal mismatch at the same author, revision mismatch, daemon-runtime-contract-digest mismatch, raw-principal-field rejection, and incompatible-version JavaScript Object Notation fixtures before the message types.
2. Define execute, operation-status, list-operations, wait, result, resume-operation-recovery, artifact-read, maintenance-result-metadata, maintenance-result-read, terminal-maintenance-preview, and terminal-maintenance-apply requests with full-word Rust identifiers and caller-created request identifiers; only operation-bearing requests carry an operation identifier.
3. Require every operation envelope to carry exact operation version, `DaemonRuntimeContractDigest`, expected `AuthorTargetIdentity`, and `SelectedEnvironmentRevision`; require recovery resume to carry the exact expected operation revision and exact expected recovery category; derive every bound/default from the typed daemon manifest, define bounded list filters plus versioned cursor, use operation/slot/`ArtifactIdentifier` plus expected digest for artifact reads, key `MaintenanceResultMetadata` only by explicit target digest plus `MaintenanceResultIdentifier`, use those values plus expected content digest for maintenance-result reads, place no operation/slot/path/offset/digest field in the metadata request, and require maintenance preview/apply to identify exactly one author-target partition.
4. Define accepted, replayed, unavailable-executor, status, list-page, progress, recovery-resume-applied/replayed with recorded source receipt and truthful current status, conditional-evidence recovery-required, conditional terminal failure, artifact-start/chunk/end, maintenance-result-metadata, maintenance-result-start/chunk/end, complete maintenance-manifest preview, maintenance-application applied/replayed receipt, scheduler capacity, persistent-capacity-exhausted and persistent-storage-backpressure with bounded usage/limit and maintenance guidance, conflict, missing-operation, transition, target/revision/digest mismatch, compatibility, and internal response variants.
5. Validate identifiers, operation version, expected target and revision, operation revision, recovery category, artifact or maintenance-result identifier and digest, byte offset, decoded chunk bound, complete maintenance manifest/digest/partition, cursor and result-descriptor bounds while rejecting unknown fields and invalid combinations. Both a successful metadata response and `MaintenanceResultStart` expose the exact target, identifier, kind, reviewed source digest, content digest, exact length, fixed `application/json`, association revision, and retention owner without bytes, path, operation, artifact, or slot; chunks remain the only maintenance response carrying bytes.
6. Serialize through one canonical writer so identical typed messages always produce identical payload bytes.

**Tests:**

- Every request and response variant accepts its valid fixture and reproduces the canonical fixture bytes, including exact recovery-resume receipt replays after later/terminal status, distinct scheduler/persistent capacity errors, list cursors, structured-result artifact descriptors, complete maintenance manifests, target-qualified maintenance-result metadata followed by reads/chunks, and applied/replayed maintenance receipts.
- Recovery fixtures accept only ExecutionCertainty for unresolved execution categories or AuthoritativeRemoteSuccess without certainty for post-success completion categories. Terminal failure fixtures additionally accept ResultUnavailable with AuthoritativeRemoteSuccess and no certainty; all kind/disposition/evidence cross-combinations fail decoding.
- Unknown fields, missing identifiers, a zero operation revision where a revision is required, result bytes embedded outside an artifact- or maintenance-result-chunk response, any digest/offset/operation/artifact/slot/path field on maintenance-result metadata, operation/slot fields on maintenance-result reads, and over-bound read requests or chunk responses are rejected.
- Unsupported operation versions, daemon-runtime digest mismatch, target mismatch including changed opaque authentication principal, and selected-environment revision mismatch have distinct bounded responses and never deserialize or route as a current request; no raw username or Cloud principal tuple is a legal envelope field.
- Sensitive command fields remain typed payload data and never appear in validation error text.

- **Done when:** `cargo test -p slingshot-local-protocol --test message` passes every versioned operation variant and rejection fixture, including conditional terminal errors, durable recovery-resume receipt replay, complete bounded maintenance manifests plus application-receipt replay and operation-free maintenance-result metadata/transfer, identity/revision refusal, deterministic artifact descriptors, and large-result exclusion, and all workspace test, clippy, and formatting gates succeed.
