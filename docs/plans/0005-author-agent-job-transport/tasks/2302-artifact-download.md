---
id: artifact-download
title: "Artifact Download"
workstream: "0023"
kind: task
depends_on:
  - bounded-structured-results
  - recovery-and-event-supervisor
gated: false
touches:
  - crates/slingshot-agent-connection/src/artifact_download.rs
  - crates/slingshot-agent-connection/src/lib.rs
  - crates/slingshot-daemon/src/operation/artifact_completion.rs
  - crates/slingshot-daemon/src/operation/mod.rs
  - crates/slingshot-agent-connection/tests/fixtures/artifact-download/**
  - crates/slingshot-agent-connection/tests/artifact_download.rs
status: done
merged_as: "d8ebbe5b54ce946306ab9a1e345d5d313c33d1c6"
---
# Artifact Download

Turn location-free author artifact metadata into one atomically published local artifact from the fixed operation/slot route only after target, length, and digest verification.

**Steps:**

1. Commit package, large canonical JSON, exact/missing/wrong terminal SubmittedCommandDigest and valid-shaped same-command/different-arguments artifact-result substitution, malformed/noncanonical/schema-invalid JSON, exact/mismatched/parameterized/duplicate Content-Type, absent/identity/duplicate/compressed Content-Encoding, HTTP/1.1/HTTP/2 equivalence, unsupported protocol/upgrade/migration, informational head, trailer declaration/empty/nonempty actual trailer, ambiguous framing, invalid HTTP/2 header semantics, trailing bytes, exact/over shared HTTP/1.1 raw/decoded and HTTP/2 encoded/decoded single-header/count/aggregate response heads, exact fixed route, empty, truncated, slot-maximum/oversized, wrong-length, wrong-digest, changed-identifier/slot/metadata, server-supplied-location rejection, route-prefix/encoding attack, interrupted, exact identity-bearing ArtifactUnavailable 404 Missing/410 RetentionExpired plus every bare/malformed/wrong-reason/wrong-generation/wrong-operation/wrong-artifact/wrong-slot variant, throttled statuses, bounded `Retry-After`, connect/header/total/idle deadlines, same-origin redirect, cross-origin redirect, capacity one-below/exact/one-above and maintenance/resume, authoritative post-success retention/integrity loss, live retry, and changed-AuthorTargetIdentity fixtures before implementation.
2. Adopt the dependency-ordered daemon operation root, declare `artifact_completion` exactly once, and construct the sole versioned route from the immutable selected snapshot's typed author base prefix, AgentOperationIdentifier, and ArtifactSlot with segment-safe encoding; never decode route material from opaque AuthorTargetIdentity, accept no server-supplied location, and disable every redirect.
3. Before transfer, require the terminal SubmittedCommandDigest to equal the persisted/recomputed value, convert the bounded logical result to Plan 0003 CommandResult, and call `validate_result_for_command`; only then persist the AuthorTargetIdentity-partitioned AgentOperationIdentifier plus remote ArtifactIdentifier/ArtifactSlot to the stable local ArtifactIdentifier mapping. Require load artifacts to use `loaded_content_json` with `application/json`, packages to use `content_package` with `application/zip`, and reject remote use of generic local `structured_result`.
4. Before issuing the artifact GET or creating a staging file, reserve exact expected length through Plan 0004 capacity accounting. Refusal publishes nothing, records manual-resume-only PersistentCapacityUnavailable/AuthoritativeRemoteSuccess with same identities/mapping/retention urgency, and sends no POST or automatic transfer. With capacity reserved, require the shared HTTP/1.1-or-HTTP/2 response-head/framing policy, reject informational heads and `Trailer` declaration before body exposure, require one parameter-free Content-Type exactly equal to the manifest media type and absent/identity Content-Encoding with automatic decompression disabled; stream authenticated identity bytes into the Plan 0004 partial artifact interface while enforcing exact expected and slot-maximum lengths, named total-transfer/inter-byte idle deadlines, and incremental ArtifactDigest. Accept completion/publication only after exact framed end proves absent actual trailers, no trailing bytes, exact length, and digest. Accept artifact 404/410 only through the closed JavaScript Object Notation ArtifactUnavailable wrapper whose status/reason and generation/agent-operation/artifact/slot fields match the persisted request; apply the remaining closed status/retry policy and disable every redirect, including same-origin.
5. For `loaded_content_json`, incrementally prove the downloaded bytes are canonical JSON satisfying Plan 0003's RepositoryJavaScriptObjectNotationResource document schema without placing the document in a structured response; publish by atomic rename and persist the complete validated command-specific logical result, including the requested path and ArtifactDescriptor, only after required schema/canonical-form, length, and digest checks match.
6. Remove partial files and release only uncommitted reservations after every failed attempt while retaining mapping/retry facts and AuthoritativeRemoteSuccess; every live/restart/capacity-resume retry reuses the same identifiers and refuses changed metadata, bytes, or author-target partition. Bare/malformed/mismatched unavailable responses remain protocol-invalid acquisition recovery. Map only validated matching persistent Missing beyond grace or RetentionExpired, or irreparable length/digest/schema invalidity after Succeeded, to ResultUnavailable/AuthoritativeRemoteSuccess, never RecoveryWindowExpired or unknown execution; RecoveryWindowExpired remains exclusive to Retired operation truth before Succeeded is known.

**Tests:**

- Exact and empty artifacts publish with expected metadata and bytes.
- Truncated, oversized, wrong-length, wrong-digest, informational, trailer-declared, actual-trailer, ambiguous-framing, invalid-HTTP/2-header, over-bound encoded/decoded-head, trailing-byte, and interrupted bodies never publish.
- Server-supplied location, route-prefix/segment escape, and same/cross-origin redirect attempts send no credential-bearing follow-up request.
- Live retry after interruption leaves one complete artifact and no partial file without daemon restart.
- Exact identity-bound 404 Missing enters grace and exact 410 RetentionExpired establishes post-success unavailability; every bare/malformed/mismatched response publishes nothing, remains recoverable, and cannot select a terminal kind/disposition.
- Capacity refusal creates no body read, staging file, published result, or automatic retry; terminal maintenance plus exact ResumeOperationRecovery acquires the same artifact if still retained and never submits the command again, while authoritative loss settles ResultUnavailable with remote success preserved.
- A second different body for a verified artifact identifier is an integrity conflict and preserves the original.
- Large JSON and opaque/non-auto-extracted package slots/media types remain distinct; wrong command digest, same-command/different-arguments substitution, or malformed/noncanonical/schema-invalid JSON never creates a mapping or publishes; and a same-name changed AuthorTargetIdentity cannot read or overwrite the prior partition's mapping.

- **Done when:** cargo test -p slingshot-agent-connection --test artifact_download passes all terminal-command-digest/request-result-correlation, strict HTTP/1.1-or-HTTP/2 informational/trailer/framing response policy, fixed-route, stable identifier/slot, target-partition, canonical-schema-valid JSON/opaque package, identity-bound unavailable-status/result mapping, deadline, redirect refusal, authentication, streaming, length, digest, cleanup, atomicity, and live/restart retry cases.
