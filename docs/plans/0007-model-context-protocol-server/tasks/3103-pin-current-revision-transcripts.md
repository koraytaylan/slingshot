---
id: pin-current-revision-transcripts
title: "Pin Current Revision Transcripts"
workstream: "0031"
kind: task
depends_on:
  - model-context-protocol-application-entry
gated: false
touches:
  - crates/slingshot-command-line/tests/current_revision_transcripts.rs
  - crates/slingshot-test-support/fixtures/model-context-protocol/current-revision/**
status: done
merged_as: "3e30e712777b436432c46fe90f8a32d5fbeadbf1"
---
# Pin Current Revision Transcripts

Pin complete revision `2026-07-28` byte sessions for discovery, registry surface, operation execution, resources, progress, cancellation, errors, and shutdown.

**Steps:**

1. Record readable input and exact output for the existing complete modern corpus plus the exact `["2026-07-28","2025-06-18"]` discovery/unsupported-version order, tools/resources-only capabilities, complete supported method/notification inventory, every reachable modern error and unreachable header-mismatch/missing-client-capability sentinel, and the applicable complete-official-schema definition/digest for every positive or malformed inbound request/notification. Include tools/list schema provenance and the exact five-required/seven-optional operation-key matrix; exact daemon-runtime/author-agent-transport/canonical-contract/annotation/five-field identity with independent drift refusal; supplied-key preservation and omitted-optional-key one-time generation across reconnect/retry; raw-byte/Draft-2020-12/typed stage ordering; canonical phrase/asset/token inputs; inline and externalized complete maintenance values; exact maintenance-result template plus fresh-process target-and-identifier metadata lookup and authenticated read lifecycle; and every revised closed configuration/FileVault/add-component/continuation failure with CLI-parity bytes.
2. Replay each session through the compiled server with a scripted daemon and independent stderr capture.
3. Validate each positive inbound request/notification against its applicable definition in the digest-verified complete official revision schema, prove malformed inputs fail that oracle while unsupported modern versions produce exact `-32022` and schema-valid unavailable methods/capabilities produce exact `-32601`, validate every stdout line against its applicable response definition, compare bytes, and assert coverage of every generated tool and operation/maintenance resource template.
4. Assert every successful JSON-RPC result, including tool-level `isError: true`, carries `resultType: "complete"`, and every list/read result carries its required cache and response metadata.

**Tests:**

- `current_revision_transcripts` byte-compares the full modern session corpus.
- Coverage assertions prove all tools, all applicable outcome tags, every missing-required-key refusal and omitted-optional-key success, exact supplied/generated identifier persistence through reconnect/retry, operation resources, exact target-qualified maintenance-result template/read contents, progress, reconnect, cancellation, both recovery-evidence forms, durable resume and maintenance apply/replay, changed-category refusal, duplicate/saturated request behavior, command-versus-observation error semantics, every legal conditional operation-terminal payload, and control/local error family occur. Maintenance reads cover fresh-process metadata lookup without cache or hash inversion, unchanged and sole checked apply-transfer Start, current preview, transfer after read-start linearization, applied/replayed receipt, restart, supersession or retirement before Start, corrupt stream, and all target/identifier/association mismatch refusals with no partial contents.
- Coverage fails if the exact ordered revision/capability/method/error inventory drifts; an inbound request/notification lacks its official-definition oracle; a runtime/transport/canonical-contract/five-field provenance drift case is absent; raw-byte/schema/typed order is not observed; an inline/externalized maintenance reference, identifier, resource text, or digest differs from CLI/Plan 0004; or any revised failure/reason/budget/canonical-input case is absent, aliased, widened, or differs from CLI structured bytes.

- **Done when:** `cargo test -p slingshot-command-line --test current_revision_transcripts` validates every inbound classification and outbound revision 2026-07-28 line against its applicable definition in the pinned complete official schema and byte-matches the exact ordered discovery/error inventory plus complete runtime/transport/canonical-schema-authenticated corpus including operation-free maintenance references and metadata-authenticated, race-checked lifecycle-valid resource reads.
