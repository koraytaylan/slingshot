---
id: pin-legacy-revision-transcripts
title: "Pin Legacy Revision Transcripts"
workstream: "0031"
kind: task
depends_on:
  - model-context-protocol-application-entry
gated: false
touches:
  - crates/slingshot-command-line/tests/legacy_revision_transcripts.rs
  - crates/slingshot-test-support/fixtures/model-context-protocol/legacy-revision/**
status: done
merged_as: "cc3e4b6295f3023882c2098b5627b2f2003cc514"
---
# Pin Legacy Revision Transcripts

Pin revision `2025-06-18` byte sessions, including the exact lifecycle and tool conversation used by fsm.

**Steps:**

1. Record two real-FSM-shaped sequences: `initialize` requesting exact `2025-06-18`, `notifications/initialized`, direct `tools/call`; and schema-valid `initialize` requesting another version, a successful result offering `2025-06-18`, `notifications/initialized`, direct `tools/call`. Record exact tools/resources-only capabilities, complete supported method/notification and closed error inventories, ping, operation and maintenance resources, progress, cancellation, lifecycle errors, and shutdown, with synthetic `tools/list` in a separate caller transcript. Bind every positive or malformed inbound request/notification to its applicable definition and digest in the complete official legacy schema.
2. Replay each session through the compiled server against the same exact daemon-runtime/author-agent-transport/canonical-contract/annotation/five-field registry provenance, five-required/seven-optional operation-key schemas, supplied-key preservation and omitted-optional-key one-time generation across reconnect/retry, raw-byte/schema/typed ordering, canonical phrase/asset/token inputs, revised closed semantic failures, receipts, recovery, inline/externalized maintenance, exact operation-free maintenance-result template plus fresh-process metadata-then-read lifecycle with unchanged or sole checked apply-transfer Start, terminal outcomes, saturation, list, and error states used by modern transcripts.
3. Validate each positive inbound request/notification against its applicable definition in the digest-verified complete official legacy schema, prove malformed inputs fail that oracle while schema-valid unsupported initialize negotiation succeeds with `2025-06-18` and unavailable-method/lifecycle cases retain their exact protocol errors, validate every output line against its applicable response definition, and compare it byte for byte.
4. Normalize only validated era decoration and compare semantic `MachineOutcomeEnvelope`, tool, and resource payloads with current-revision fixtures.

**Tests:**

- `legacy_revision_transcripts` validates every inbound classification and byte-compares the complete legacy corpus, exact-version and fallback-negotiated real-FSM direct-call exchanges, and separate synthetic list exchange.
- Cross-revision assertions prove normalized semantic payloads, contextual `isError` values, missing-required-key refusals, omitted-optional-key successes, and supplied/generated operation-identifier reuse are equal for all applicable outcome tags while legacy bytes omit every modern-only result/cache member.
- Exact failure literal/fields, runtime/transport/command-schema provenance, and inline/externalized maintenance reference/digest/resource text and metadata-authenticated lifecycle normalize identically across revisions and match CLI JSON/Plan 0004; both eras prove no cache or hash inversion and only the checked apply owner/revision transition. Only validated era decoration may differ. One-at-a-time runtime, transport, canonical-contract, limits, and either role-schema drift fails before execution in both eras.

- **Done when:** `cargo test -p slingshot-command-line --test legacy_revision_transcripts` validates every inbound classification and outbound line against its applicable definition, byte-matches exact-version and schema-valid-unsupported-version negotiated real-FSM revision 2025-06-18 direct-call conversations, and proves normalized semantics including operation-free maintenance references and metadata-authenticated, race-checked lifecycle-valid reads equal the modern corpus without modern-only members.
