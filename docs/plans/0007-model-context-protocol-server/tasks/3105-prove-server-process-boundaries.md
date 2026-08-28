---
id: prove-server-process-boundaries
title: "Prove Server Process Boundaries"
workstream: "0031"
kind: task
depends_on:
  - pin-current-revision-transcripts
  - pin-legacy-revision-transcripts
gated: false
touches:
  - crates/slingshot-command-line/tests/model_context_protocol_process_boundaries.rs
  - crates/slingshot-test-support/fixtures/model-context-protocol/process-boundaries/**
status: planned
merged_as: ""
---
# Prove Server Process Boundaries

Prove stdout hygiene, active-request bounds, output-pressure fail-stop, nonblocking/drop-capable stderr, secret redaction, bounded shutdown, and absence of server-initiated requests against the compiled process.

**Steps:**

1. Commit sentinel-secret and fault scenarios for parsing, exact revision/discovery/legacy negotiation, daemon bootstrap, runtime/transport provenance, authentication, five-required/seven-optional operation keys, supplied-key preservation, omitted-optional-key one-time generation across forced daemon reconnect/retry, operation, result, fresh-process maximum maintenance-resource metadata-then-read, unchanged and exact-apply-transfer Start, read-start linearization, maintenance association supersession/retirement/corruption, progress, cancellation, duplicate active identifiers, silent-request active-limit saturation, reuse before/after writer acknowledgement, externally terminated protocol process, parent-never-reads stdout, closed stdout, full output queue, parent-never-reads stderr, closed stderr, full diagnostic queue, concurrent response/progress/diagnostic producers, end of input, output-failure/cancellation/terminal/write-acknowledgement/end-of-input races, responsive current-nonce cleanup, unresponsive owned child, stale nonce, owner replacement, and process-identifier reuse beside an unowned sentinel.
2. Compose Plan 0006's Plan-0001/Plan-0004-bound `slingshot-test-support` process harness with scripted daemon endpoints and run both revisions with raw stream capture, forced concurrency, independently controllable stdout/stderr consumption or closure, and deadlines derived from the named production bounds. Continuously retain each owned child/native handle or supervision channel from creation through reap; use current-nonce cooperative `daemon.stop` for responsive daemons and that retained instance primitive alone for unresponsive owned children.
3. Fill the operating-system stdout pipe without reading, close its read end separately, and force concurrent producers until queue pressure expires. Assert intake stops, the child exits within the output-failure shutdown deadline, all local waiters detach, active reservations release once after suppression/detachment, daemon/agent cancellation counts remain zero, and durable operations remain observable from a new client.
4. In independent cases fill or close stderr while stdout stays healthy. Saturate the diagnostic queue and trigger ordinary plus fail-stop diagnostics; assert records are bounded/dropped, a later valid request completes, and normal/fail-stop process exit never waits for or joins the blocked diagnostic writer.
5. Parse every newline-terminated stdout segment, classify message direction, require any final unterminated prefix to be terminal with no later byte, scan all captures for sentinel encodings, and assert complete instance-bound child cleanup. Treat process identifiers as diagnostics only; never enumerate descendants by identifier and never check an identifier and later signal it.

**Tests:**

- `model_context_protocol_process_boundaries` exercises every fault in both revisions and proves duplicate/saturated requests never dispatch, operation-key required/optional classification is exact, an omitted optional key is generated once and never changes across forced reconnect/retry, reuse respects writer/cancellation release, a fresh process resolves the exact-maximum maintenance digest through target-and-identifier metadata without cache/hash inversion, accepts only unchanged or checked apply-transfer Start, and emits one complete bounded line or no line; external termination or slow/closed/full stdout detaches waiters without cancelling durable operations or changing maintenance association lifecycle, and stale nonce/handle/process-identifier evidence cannot affect a replacement.
- Wall-clock assertions with tolerance derived from the named deadlines prove the compiled child cannot hang on unread stdout or stderr; race fixtures prove exactly one shutdown cause, once-only reservation release, and no post-failure response/progress line.
- Blocked/closed stderr cases prove diagnostic loss cannot delay a valid following protocol response or either shutdown path and never contaminates stdout.
- Positive controls prove stdout and secret scanners fail on deliberate contamination; cleanup sentinels fail on descendant-identifier discovery, check-then-signal-by-identifier, or loss of the retained instance handle.

- **Done when:** `cargo test -p slingshot-command-line --test model_context_protocol_process_boundaries` proves the compiled server bounds/linearizes active requests, exits boundedly under unread/closed/saturated stdout or stderr, drops diagnostics without delaying service, detaches without remote cancellation, emits only complete valid lines plus at most one terminal stdout prefix, and leaks no request, owned child, or secret while cleanup cannot signal a replacement.
