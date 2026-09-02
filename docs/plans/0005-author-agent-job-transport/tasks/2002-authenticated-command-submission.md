---
id: authenticated-command-submission
title: "Authenticated Command Submission"
workstream: "0020"
kind: task
depends_on:
  - author-cross-site-request-forgery-protection
  - capability-discovery
gated: false
touches:
  - crates/slingshot-agent-connection/src/command_submission.rs
  - crates/slingshot-agent-connection/src/lib.rs
  - crates/slingshot-agent-connection/tests/fixtures/command-submission/**
  - crates/slingshot-agent-connection/tests/command_submission.rs
status: done
merged_as: "48b8c80db5954d79d41955da86ddaa8bb6da20dc"
---
# Authenticated Command Submission

Submit one canonical command to author and accept only a bounded response tied to the same durable operation.

**Steps:**

1. Commit accepted, duplicate, Retired, exact/over agent operation/execution-detail/subscription/event/snapshot/result/artifact capacity no-effect refusal, event-before-response, changed/zero/noncanonical-generation-before-retry, changed-command/selected-contract/digest conflict including independent canonical-contract, each annotation/role, and limits-only changes, same-command/different-validated-arguments digest vectors, wrong/missing acceptance/duplicate/Retired/semantic-or-capacity-rejection selected-identity/digest echoes, wrong-subscription/target, valid continuation, continuation plus offset/limit, all exact malformed/integrity/wrong-query/wrong-target/expired precedence cases, request-start-relative retention with delayed/equality response, 400/403/422 rejection, 409 conflict, 408/429/500/502/503/504 same-generation-identifier retry, bounded `Retry-After`, every distinct DNS/TCP/TLS/request/header/body deadline checkpoint, HTTP/1.1/HTTP/2 equivalence, unsupported protocol/upgrade/migration, informational head, trailer declaration/empty/nonempty section, ambiguous framing, invalid HTTP/2 header semantics, over-bound encoded/decoded head, trailing-byte, malformed/oversized/wrong-content-type/redirect/empty-job-identifier/remote-error fixtures before implementation.
2. Derive ExpectedArtifactManifest before POST from Plan 0003 declarations. Select/persist AuthorAgentTransportContractDigest, exact CommandCanonicalJsonContractDigest, both schema-root annotations/role digests, and the unchanged five-field identity. Authenticate those artifacts, validate exact raw argument bytes under `slingshot.command-canonical-json/1`, apply Draft 2020-12 decoded shape, then typed conversion. Compute SubmittedCommandDigest over transport digest, five fields plus the separately ordered canonical-contract digest, complete defaulted canonical arguments, and manifest; atomically persist every value before building the exact same canonical request bytes.
3. Obtain one fresh AuthorCrossSiteRequestForgeryToken immediately before the attempt; set exact external `CSRF-Token`, origin-derived `Referer`, and Idempotency-Key headers; reject caller override/duplicates; and require submission to atomically register the stable subscription plus same-identifier mapping and reserve the complete worst-case execution-detail/event/snapshot/result/artifact retention branch before creating a Sling Job.
4. Discover/validate current generation, transport/canonical-contract/dual-annotation/unchanged-five-field identities, logical-execution guarantee, and capacity/rotation before every recovery submission; recompute all persisted provenance and send no derivation preimages. Sample monotonic request-start before network work and reduce every accepted relative retention by total elapsed time at trailer-free exact complete-body receipt, with equality expired. Validate HTTP version, sole final head, status/media/framing/trailer absence/body boundary, and every echo before persisting Accepted, Duplicate, or Retired. Accepted/Duplicate may carry the bounded sorted set of exact physical Sling Job identifiers for the one logical operation; empty, duplicate, unsorted, mismatched, or over-bound sets are invalid. Matching Retired sends no replacement work; validated capacity refusal is no-effect Rejected/AuthoritativeNonExecution.
5. Apply the shared route/status/deadline policy. After any request byte may have reached author, every unvalidated status, informational head, trailer declaration/section, framing or trailing-byte failure, media/coding, malformed/oversized/truncated body, version, identity/generation/partition, registration, SubmittedCommandDigest, retention, empty identifier, or unknown-field outcome persists SubmissionUnknown and enters generation-gated same-identifier/digest lookup-first reconciliation. Only proven zero-byte transport failure or a completely validated identity/generation/digest-bearing nonexecution rejection can use ConfirmedNotExecuted.
6. Map remote and transport failures into a closed error taxonomy without promoting arbitrary response text into an error code.

**Tests:**

- Every Plan 0003 command receives the exact canonical artifact digest, both schema-root annotations/role digests, exact canonical arguments, all five selected-contract fields, empty/load/package manifest, and the one exactly derived SubmittedCommandDigest. Contract-only, annotation-only, arguments, version, limits, either schema, or manifest drift under the same generation-scoped identifier independently conflicts without changing the five-field identity shape.
- Accepted and duplicate responses return the same logical-operation provenance and canonically sorted known physical identifier set; crash recovery may enlarge that set within the manifest bound but cannot authorize another effect. Only a fully matching Retired response maps to RecoveryWindowExpired and cannot create replacement work.
- An event arriving before the response resolves the target-partitioned local record by AgentOperationIdentifier without a learned job identifier.
- Conflict is distinct from retryable transport failure and creates no second fake-author job.
- Agent-capacity refusal names only its closed capacity discriminator, proves no operation/execution-detail/subscription/event/snapshot/result/artifact partial reservation or Sling Job, performs no eviction/rotation, and settles as authoritative nonexecution.
- Throttling, retryable statuses, and every post-byte phase deadline persist their bounded delay and retry only the same explicit generation and opaque identifier pair; a pre-byte failure remains distinguishable. A delayed response reduces retention from request-start and equality cannot admit or promise recovery time.
- Reused profile/environment/local identifiers under a changed AuthorTargetIdentity derive a different subscription/agent-operation partition and never replay the prior target's work.
- A generation change blocks recovery submission and cannot pair the persisted identifier with the new generation or derive a replacement; known Sling Job recovery uses snapshot lookup and an ambiguous identifier without such evidence fails closed.
- Discovery submissions use exact Initial or Continuation shape; Continuation preserves opaque bytes, carries no offset/limit, reuses the token-protected originating limit without reapplying Initial offset, and FakeAuthor validates target/version/query binding with distinct stable failures without implying snapshot consistency.
- Redirect, upgrade/migration, informational head, trailer declaration/section, ambiguous framing, invalid HTTP/2 semantics, over-bound encoded/decoded head, trailing bytes, wrong/duplicate media or coding, malformed/truncated/oversized body, unsupported version, wrong identity/generation/partition/SubmittedCommandDigest, empty identifier, and unknown fields after POST bytes preserve SubmissionUnknown; FakeAuthor cases with and without a created job converge only through a digest-matching lookup and never silently terminalize or duplicate it.
- Errors and captured diagnostics contain no authentication value or unbounded response body.

- **Done when:** cargo test -p slingshot-agent-connection --test command_submission passes the complete canonical-artifact/dual-annotation/unchanged-five-field command/digest/substitution matrix, strict HTTP/1.1-or-HTTP/2 informational/trailer/framing boundaries, request-start retention and split-phase deadlines, and every accepted response is bounded, authenticated, author-only, target-partitioned, subscription-registered, recovery-bounded, and tied to its AgentOperationIdentifier plus SubmittedCommandDigest.
