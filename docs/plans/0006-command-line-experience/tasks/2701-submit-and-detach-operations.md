---
id: submit-and-detach-operations
title: "Submit And Detach Operations"
workstream: "0027"
kind: task
depends_on:
  - expose-asset-query-commands
  - expose-configuration-and-page-query-commands
  - expose-content-commands
  - expose-package-and-replication-commands
  - expose-page-mutation-commands
  - manage-daemon-processes
gated: false
touches:
  - crates/slingshot-command-line/src/operation_submission.rs
  - crates/slingshot-command-line/tests/operation_submission.rs
status: done
merged_as: "90edc7e5969d3e67675b1c7d2c7f30af99db938d"
---
# Submit And Detach Operations

Submit any catalog request through the namespaced daemon and either return its durable receipt or enter attached observation.

**Steps:**

1. Commit local-protocol transcripts for the exact seven auto-generated intrinsically-idempotent configuration/discovery identifiers, the five required caller-supplied non-idempotent identifiers including content load, accepted, rejected, duplicate, current/mismatched target/revision, disconnected-before-receipt, process-loss/rerun, attached, and detached submissions.
2. Resolve every command binding from the registry. Load exact canonical `schemas/command-canonical-json-1.json`, require format `slingshot.command-canonical-json/1`, recompute its digest, require the command-schema manifest value and both role roots' `x-slingshot-canonical-json-contract-sha256` annotations to equal it, recompute the annotated role digests, and then require stable wire name, exact semantic version `1.0.0`, canonical command-limits digest, and both exact role-schema digests before daemon access. Also recompute exact canonical `policy/author-agent-transport-contract-1.json`, format `slingshot.author-agent-transport-contract/1`, against its sidecar for later current-operation provenance comparison. Neither digest adds a field to `SelectedCommandContractIdentity`.
3. Read intrinsic-idempotency from that validated descriptor, reject a missing caller key for every non-intrinsically-idempotent operation before daemon access, and generate an identifier exactly once only for an intrinsically idempotent operation.
4. Canonically serialize each constructed argument and validate its exact raw bytes and per-role ordered-array inventory under `slingshot.command-canonical-json/1`, then its decoded tree under the ordinary Draft 2020-12 argument schema, then its typed/cross-field constructor. Reject rather than parse-and-reserialize any noncanonical raw property/predicate input. Only after all three stages implement common submission with the selected identifier, current `AuthorTargetIdentity`, `SelectedEnvironmentRevision`, and exact expected `DaemonRuntimeContractDigest`; require any current agent-backed durable receipt provenance to carry the exact installed `AuthorAgentTransportContractDigest`; then render detached accepted/replayed admission and revision or transition into the observer without operation-specific branches.
5. Refuse an existing daemon whose immutable identity or loaded revision differs, render explicit stop/start guidance, and send no submission to that daemon.
6. Drop the first independent client instance after daemon admission but before receipt observation, create a fresh client instance with the same required key, and assert one durable operation/executor invocation; workstream 0028 repeats this boundary with compiled processes.

**Tests:**

- `operation_submission` replays every transcript and pins exact accepted/replayed `operation_receipt` envelopes and errors.
- In-process reconnect proves a generated read identifier is stable; lost-response/new-process rerun proves the required mutation key addresses one existing operation and creates no duplicate work.
- The catalog-derived seven-idempotent/five-non-idempotent inventory is exhaustive; omitting the key for any of the five, including read-classified content load, produces zero daemon/process/network calls.
- A stale daemon-runtime contract blocks the versioned local request; a stale author-agent-transport digest blocks compatible receipt/result presentation; and a stale canonical-contract artifact/manifest/annotation, command version, limits digest, argument digest, or result digest produces zero execution/network calls and cannot be hidden by otherwise matching wire name or schema shape. Contract-only drift keeps the five descriptor fields and both role bytes/digests fixed while mutating the separately loaded contract artifact and must still fail.
- Ordering cases prove raw-byte/member/array canonicality fails before Draft 2020-12 shape, standard-schema failure occurs before typed construction, and typed/cross-field failure occurs last; no earlier rejection invokes the daemon.
- Mismatch cases prove zero execution requests follow a failed target/revision handshake.

- **Done when:** `cargo test -p slingshot-command-line --test operation_submission` proves exact daemon-runtime and author-agent-transport provenance, authenticated canonical-contract/annotation/five-field provenance, raw-byte/schema/typed ordering, exact seven/five registry matrix, every non-intrinsically-idempotent operation's caller-key requirement before access, and a post-admission lost-response rerun with that key executes exactly once, while only the seven intrinsically idempotent reads may auto-generate.
