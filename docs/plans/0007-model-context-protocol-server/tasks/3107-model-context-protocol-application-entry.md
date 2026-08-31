---
id: model-context-protocol-application-entry
title: "Model Context Protocol Application Entry"
workstream: "0031"
kind: task
depends_on:
  - stream-progress-and-detach-cancellation
gated: false
touches:
  - crates/slingshot-command-line/src/application.rs
  - crates/slingshot-command-line/src/invocation.rs
  - crates/slingshot-command-line/src/lib.rs
  - crates/slingshot-command-line/src/main.rs
  - crates/slingshot-command-line/src/model_context_protocol/application.rs
  - crates/slingshot-command-line/tests/model_context_protocol_application.rs
  - "crates/slingshot-command-line/tests/fixtures/model-context-protocol/application/**"
status: done
merged_as: "478ce34ad1e310715d8f0be2099ee52a04af4b53"
---
# Model Context Protocol Application Entry

Compose the documented `slingshot model-context-protocol serve` parser leaf into the complete target-bound standard-stream server.

**Steps:**

1. Commit application fixtures for explicit/default target selection; rejection of `--output`, `--detach`, `--operation-key`, target digest, and ordinary operation options; exact/mismatched `DaemonRuntimeContractDigest`; exact/mismatched `AuthorAgentTransportContractDigest`; target/revision mismatch; exact ordered modern discovery/error startup; legacy exact/fallback-negotiated startup; operation-free maintenance-result template plus fresh-process target-and-identifier metadata-then-read across reconnect; unchanged and sole checked apply-transfer Start; end of input; startup failure; active-request saturation; queue-pressure/output-write timeout; closed stdout; blocked/closed stderr; output-failure/end-of-input race; stdout contamination; and ordinary-command separation.
2. Verify the scaffolded `model_context_protocol/mod.rs` still exposes exactly these thirteen already crate-reachable feature leaves in the independently committed inventory order: `active_request_registry`, `application`, `current_stateless_revision`, `legacy_initialized_revision`, `operation_execution`, `progress_and_cancellation`, `protocol_diagnostics`, `resource_catalog`, `result_projection`, `schema_projection`, `size_budget`, `standard_stream_transport`, and `tool_catalog`. A bidirectional source/declaration test rejects a missing, surplus, duplicate, renamed, or undeclared leaf; this task does not reopen the parent module.
3. Extend the Plan 0006 invocation/application tree with the exact profile/environment-only serve leaf and compose typed daemon-runtime and author-agent-transport contracts, target resolution, exact runtime-digest/target/revision-checked daemon startup, protocol transport, active-request registry, era handlers consuming the one `["2026-07-28","2025-06-18"]` authority, registry surface including the target-qualified maintenance-result metadata resolver and authenticated reader, operation coordination, drop-capable protocol diagnostics, sole stdout writer, and once-only shutdown coordinator.
4. Keep ordinary command rendering inactive while the server owns standard streams; on output failure stop intake, detach all waiters, release reservations after suppression/detachment, abandon unstarted output, and return Plan 0006's local transport process exit within the named shutdown deadline without waiting indefinitely for a blocked writer, input reader, or diagnostic sink.
5. Prove the production binary entry supplies real boundaries and every injected-boundary case reaches exactly one server application instance.

**Tests:**

- `model_context_protocol_application` drives the top-level executable entry for both revisions and verifies the exact thirteen-leaf compiled inventory, ordered supported-revision authority, legacy fallback negotiation, expected `DaemonRuntimeContractDigest`, current `AuthorTargetIdentity`, and `SelectedEnvironmentRevision` reach every versioned daemon handshake/request while current agent-backed provenance matches `AuthorAgentTransportContractDigest` before serving execution; maintenance read uses exact operation-free `MaintenanceResultMetadata` and `MaintenanceResultRead` routes keyed by the target-qualified URI without prior process state or hash inversion.
- Parser and dispatch call counts prove forbidden ordinary flags fail before external access and the serve leaf cannot fall through to ordinary command rendering or start more than one server.
- Startup and shutdown failures produce classified stderr/exit behavior; output failure produces no subsequent stdout byte, changes no durable operation state, and cannot fall back to ordinary rendering.

- **Done when:** `cargo test -p slingshot-command-line --test model_context_protocol_application` proves the profile/environment-only `slingshot model-context-protocol serve` rejects output/detach/operation-key and ordinary overrides, wires exactly the thirteen declared leaves, composes exactly one runtime/transport/target/revision-checked dual-era server with the sole ordered revision authority and operation-free metadata-authenticated maintenance reader, reserves stdout exclusively for protocol, and bounds queue/write/diagnostic failure with once-only detach/release.
