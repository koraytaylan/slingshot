---
id: bounded-server-sent-event-decoder
title: "Bounded Server-Sent Event Decoder"
workstream: "0021"
kind: task
depends_on:
  - authenticated-command-submission
gated: false
touches:
  - crates/slingshot-agent-connection/src/server_sent_event_decoder.rs
  - crates/slingshot-agent-connection/src/lib.rs
  - crates/slingshot-agent-connection/tests/fixtures/server-sent-events/**
  - crates/slingshot-agent-connection/tests/server_sent_event_decoder.rs
status: done
merged_as: ""
---
# Bounded Server-Sent Event Decoder

Decode the author event stream incrementally without allowing one line, event, identifier, or partial connection to allocate unbounded memory.

**Steps:**

1. Commit exact single `text/event-stream` with absent/UTF-8 charset and identity coding plus wrong/duplicate/conflicting media/coding, single-line, multiple-data-line, comment-heartbeat, explicit JobEvent subscription/generation, terminal AuthorAgentTransportContractDigest, separate CommandCanonicalJsonContractDigest, both schema-root annotations/role digests, unchanged-five-field SelectedCommandContractIdentity, and SubmittedCommandDigest; independently missing/wrong transport, canonical-contract, annotation, role, generation, or terminal correlation values including contract-only and limits-only mismatch; independent cursor and per-job-sequence, event-before-response, unassociated-operation, stale-operation, interleaved-job, wrong-subscription, unknown Server-Sent Event field, carriage-return, split-byte, missing-final-blank-line, malformed-data, over-line, over-event, and over-identifier fixtures before implementation.
2. Implement an incremental ServerSentEventDecoder using named limits for line bytes, event bytes, and identifier bytes.
3. Join multiple data fields with newline, ignore unknown fields, treat comments as heartbeat activity, and emit only blank-line-terminated events.
4. Require the shared HTTP/1.1-or-HTTP/2-only encoded/decoded response-head policy, rejection of an informational head or `Trailer` declaration, and exact live-stream media/coding policy before decoding; parse completed data through the closed validated JobEvent wire wrapper. Require explicit DaemonSubscriptionIdentifier and AgentEventStoreGeneration to equal the request, then authenticate AuthorAgentTransportContractDigest, separate CommandCanonicalJsonContractDigest, both selected schema-root annotations/role digests, unchanged-five-field SelectedCommandContractIdentity, and SubmittedCommandDigest in every terminal data object before exposing it. Parse the Server-Sent Event identifier as the separate EventStreamCursor, and never infer any identity, generation, artifact, contract, digest, or sequence from another. An undeclared actual trailer is a transport failure handled by reconnection and never a Server-Sent Event field or cursor fact.
5. Return a bounded protocol error and discard partial state after malformed or over-bound input.

**Tests:**

- Valid fixtures decode identically for every tested byte-chunk partition.
- Multiple data fields, comments, carriage returns, and unknown fields follow the specified contract.
- Interleaved, unassociated, and event-before-response operations retain independent per-job sequences and distinct monotonic cursors under one requested subscription/generation.
- A wrong-subscription event is rejected before cursor persistence; an unknown Server-Sent Event field remains ignored under the protocol field rules.
- End of stream without a terminating blank line emits no partial event.
- Values at each named limit succeed and values immediately above it fail without further allocation.
- Malformed JSON, a missing/surplus/wrong generation field, missing/malformed/surplus terminal SubmittedCommandDigest, and every other protocol-invalid JobEvent value return stable bounded errors rather than panicking or advancing a cursor.

- **Done when:** cargo test -p slingshot-agent-connection --test server_sent_event_decoder passes all finite-head, filtered-subscription, explicit-generation/digest-bound-terminal schema, identity, event-before-response, fixture partition, and allocation-bound cases.
