---
id: result-window
title: "Result Window"
workstream: "0009"
kind: task
depends_on:
  - command-module-scaffold
gated: false
touches:
  - crates/slingshot-domain/src/command/result_window.rs
  - crates/slingshot-domain/src/command/discovery_budget.rs
  - crates/slingshot-domain/tests/fixtures/commands/result-window.jsonl
  - crates/slingshot-domain/tests/fixtures/commands/discovery-budget.jsonl
  - crates/slingshot-domain/tests/result_window.rs
  - crates/slingshot-domain/tests/discovery_budget.rs
status: done
merged_as: "73fd2680917064fdd0be7f7c9e53c355134ec3d9"
---
# Result Window

Discovery commands need one bounded pagination contract so no caller or transport invents an unbounded result request.

**Steps:**

1. Commit fixtures for default/explicit Initial, valid Continuation, zero/maximum/over-maximum/overflow Initial limit and offset, matching-row versus raw-candidate offset counting, skipped-match charges, repository exhaustion while skipping, skip/computation/page ties, Continuation plus zero/nonzero offset, Continuation plus default/explicit limit, missing/surplus fields, and the complete language-neutral continuation protected-header/payload/tag/failure vector inventory expected from Plan 0005 before implementation.
2. Define ResultOffset, ResultLimit, ContinuationToken, closed Initial/Continuation ResultWindow, DEFAULT_RESULT_LIMIT, MAXIMUM_RESULT_LIMIT, MAXIMUM_RESULT_OFFSET, and MAXIMUM_CONTINUATION_TOKEN_BYTES with validated constructors.
3. Make an omitted window resolve to Initial with zero offset/named default limit; reject zero/over-maximum Initial limit; reject every offset or limit field on Continuation.
4. Serialize exactly `mode: initial` with `offset`/`limit` or `mode: continuation` with `continuation_token`, rejecting alternate literal, missing, unknown, and surplus fields.
5. Keep ContinuationToken opaque to Rust, non-empty, control-free, and bounded. Pin `slingshot.command-arguments-canonical/1` as the validated argument object with exact `result_window` omitted under the raw-byte and typed ordered-collection rules later inventoried by `slingshot.command-canonical-json/1`. Commit exact `slingshot.continuation-token/1` vectors: canonical protected header with `hmac_sha256` and bounded key identifier; payload with format, issued/expires UTC Unix milliseconds, command wire name, exact `1.0.0`, AuthorTargetIdentity digest, normalized non-window-argument digest, originating Initial limit, and bounded command-specific resume key; unpadded base64url framing; role-tagged HMAC input; lifetime/skew relations; and largest-envelope fit. Assign generation, persisted 256-bit HMAC-SHA-256 key material/identifier, current-plus-previous rotation, deployment-durable authority, least-authority access, secret-at-rest protection, synchronization, crash-safe replacement, expiry, and restart-stable validation exclusively to Plan 0005. Plan 0003 remains provider/topology/storage/profile-neutral, blesses no physical storage primitive, and neither stores keys nor decodes/synthesizes tokens.
6. Pin exact validation precedence and closed no-partial failures consumed by Plan 0005: malformed framing/header/key-identifier shape is `continuation_token_malformed`; unknown key or constant-time tag mismatch is `continuation_token_integrity_invalid`; only after authentication, malformed payload/time/limit/resume shape is `continuation_token_malformed`; target mismatch is `continuation_token_wrong_target`; command/version/non-window arguments mismatch is `continuation_token_wrong_query`; only an otherwise matching authentic token at/after expiry is `continuation_token_expired`. Every failure contains only `failure`. A success reuses its protected Initial limit, resumes strictly after exact `{"repository_path":...}`, and never reapplies offset.
7. Define page-at-a-time ordering and offset algorithm without a repository-snapshot guarantee. Initial offset counts fully evaluated matches only. Every raw candidate and property/criterion used to decide a skipped match receives normal computation charges; the skip itself receives no result-byte charge. Exhaustion after only skipped matches is an empty terminal page, computation exhaustion while skipping is the normal no-partial failure, emitted matches begin only after the exact offset, and Continuation resumes after the last emitted sort key without reapplying offset.
8. Implement the common DiscoveryExecutionBudget value/failure using only manifest-owned constants, exact cancellation/time/charge/offset/admission order at pre/post repository-call and canonical-output cooperative boundaries, checked arithmetic, late-return/skip/page-completion tie rules, and literal closed failure shape from the architecture. State explicitly that the clock cannot preempt one blocking call.

**Tests:**

- Default construction produces Initial with named default limit and zero offset.
- Zero and over-maximum limits return their dedicated errors.
- Maximum offset and arithmetic overflow cases never panic.
- Offset zero/max/over-max cases pin the named bound. Nonmatching candidates do not consume offset; matching rows do, after their ordinary candidate/property/criterion charges and before result-byte construction. Exact exhaustion while skipping returns empty terminal only on repository exhaustion and otherwise the shared no-partial budget failure.
- Continuation tokens reject empty, controls, and over-bound strings; token plus zero/nonzero offset or default/explicit limit has a dedicated closed-shape error.
- Contract fixtures independently pin every protected-header/payload field and bound, canonical non-window argument bytes, role-tagged HMAC input/tag, unpadded base64url, lifetime/skew, key rotation overlap, deployment-durable-authority/least-authority/no-silent-replacement requirements without prescribing a physical provider or storage primitive, maximum token envelope, and the exact malformed/integrity/wrong-target/wrong-query/expired precedence expected from Plan 0005; they explicitly permit concurrent repository mutation between strictly ordered pages.
- Continuation vectors prove every later page uses the protected originating Initial limit with no caller limit and does not reapply the Initial offset.
- Tie vectors prove cancellation/time/count failures win before a candidate match can consume offset, offset consumption wins before result construction, and an admitted page completion wins before another repository charge; skipped rows never become a continuation resume key.
- Every common counter and cooperative duration boundary has below/at/above vectors; maximum charges succeed, the next charge fails, exact duration expiry fails, a repository call returned at/after expiry becomes `execution_duration` before its payload/error is interpreted, completed-page tie wins before another repository charge, arithmetic never wraps, and no failure carries partial matches/token.
- Cancellation before a call and cancellation observed after a fake blocked call returns both stop the exact call trace and publish no result/token; the fixture makes no claim that the clock or cancellation preempts a call that has not returned.
- Every valid fixture survives canonical serialization and deserialization unchanged.

- **Done when:** cargo test -p slingshot-domain --test result_window and cargo test -p slingshot-domain --test discovery_budget pass all default, boundary, overflow, continuation/resume-limit exclusivity, exact Plan 0005 continuation binding/failure vector inventory, common cooperative pre/post-call execution-accounting, late-return/cancellation trace, tie, failure-shape, and round-trip cases without implementing token cryptography or making a blocking-call preemption claim.
