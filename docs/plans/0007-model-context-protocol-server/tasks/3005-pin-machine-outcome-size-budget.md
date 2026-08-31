---
id: pin-machine-outcome-size-budget
title: "Pin Machine Outcome Size Budget"
workstream: "0030"
kind: task
depends_on:
  - preserve-structured-result-parity
gated: false
touches:
  - crates/slingshot-command-line/src/model_context_protocol/size_budget.rs
  - crates/slingshot-command-line/tests/model_context_protocol_size_budget.rs
  - "crates/slingshot-test-support/fixtures/model-context-protocol/size-budget/**"
status: done
merged_as: "b27fde2e8bce83de2a238d9d13697b163b6ff2da"
---
# Pin Machine Outcome Size Budget

Prove every local structured outcome can be wrapped and duplicated by the standard-stream protocol without exceeding a hidden transport bound.

**Steps:**

1. Commit byte-boundary fixtures for every existing branch plus the largest legal revised configuration/FileVault/add-component/continuation semantic failure object, each of Plan 0006's four closed interruption local-error variants, the exact opaque continuation input bound, runtime/transport/command-schema/limits provenance, the largest inline maintenance preview/receipt, complete over-inline maintenance bytes and their operation-free `maintenance/results/{maintenance_result_identifier}` references, target-and-identifier metadata lookups, authenticated read starts, and reads, modern/legacy decoration, text duplication, resources, and one byte/item above every applicable limit.
2. Define named constants for inline results, machine envelope, worst-case escaped canonical text, optional resource link, protocol decoration, target-qualified maintenance URI, Plan 0004 maximum maintenance metadata response, maximum maintenance read-start message, maximum maintenance-result document bytes, worst-case resource-text escaping, complete maintenance-read response, standard-stream messages, queued-output bytes, and the pinned FSM structured acknowledgement cap; add compile-time assertions for the disposition and full sum inequalities.
3. Require `MAXIMUM_MACHINE_OUTCOME_ENVELOPE_BYTES` to be strictly below the pinned 4096-byte cap. Validate every CLI envelope branch including the maximum interruption local error against that base bound; prove the three CLI-signal-only variants satisfy no Model Context Protocol output schema. Validate every protocol-applicable complete/error/nonterminal-status—including both maximum recovery-evidence forms and terminal `ResultUnavailable`—plus largest-inline maintenance and fixed maintenance-reference envelopes through both revision decorators, and reject any message whose structured envelope plus escaped text duplicate, optional link, and era decoration exceeds the standard-stream bound. Make no assertion that a complete maximum maintenance manifest or receipt fits below 4096 bytes.
4. Prove any inline command logical value that would violate the envelope bound is represented by the daemon-created canonical `application/json` operation `structured_result` descriptor/access entry in both CLI and protocol output. Prove an over-inline maintenance value instead uses Plan 0004's operation-free target-qualified maintenance-result association and reference. Verify the exact complete bytes remain out of band from the tool envelope, the descriptor content digest equals the reviewed preview digest or exact applied/replayed receipt-byte digest, each exact local metadata response and authenticated read-start message fits its Plan 0004 bound, and a separate `resources/read` response containing one exact canonical JSON text document fits the standard-stream message and queued-output byte bounds after worst-case escaping and era decoration.
5. Prove artifact bytes never enter an envelope or resource message; leave synthetic executor-over-cap behavior to Plan 0008 compatibility fixtures.

**Tests:**

- `model_context_protocol_size_budget` checks each named bound immediately below, at, and above its limit for modern and legacy messages.
- Compile-time and runtime assertions prove accepted inline values plus envelope overhead fit; the maximum interruption local error, largest inline maintenance value, maintenance-result reference, both recovery-evidence branches, and every conditional terminal branch including result unavailable remain strictly below 4096 bytes. Complete over-inline maintenance bytes remain canonical and digest-identical out of band from the tool outcome, exact metadata and read-start messages fit their local bounds, and the separately bounded one-content `resources/read` response fits one standard-stream message and queued-output capacity at the exact maximum in both eras; next-byte/next-decoration cases fail before enqueue.
- Maximum artifact metadata fits while any artifact body marker remains absent from all serialized cases.
- Maximum revised failure objects and provenance fit without dropping registered fields; an over-bound opaque token fails input validation rather than entering an output envelope.

- **Done when:** `cargo test -p slingshot-command-line --test model_context_protocol_size_budget` proves every CLI envelope including phase-specific interruption remains strictly below 4096, CLI-signal-only variants cannot enter protocol output, and every conforming protocol outcome—including inline or operation-free externalized complete maintenance and conditional recovery/result-unavailable terminal branches—yields identical CLI/structured/text envelope/reference bytes and digests whose duplicated/link-decorated modern and legacy messages fit the named stream bound, while an exact-maximum maintenance resource read independently fits the message/queue bounds and the next unit is refused.
