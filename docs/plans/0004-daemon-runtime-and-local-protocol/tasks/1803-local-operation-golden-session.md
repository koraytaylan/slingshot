---
id: local-operation-golden-session
title: "Local Operation Golden Session"
workstream: "0018"
kind: task
depends_on:
  - concurrent-client-process-suite
  - local-protocol-golden-transcripts
gated: false
touches:
  - crates/slingshot-development/tests/local_operation_session.rs
  - "crates/slingshot-development/tests/fixtures/local-operation-session/**"
status: planned
merged_as: ""
---
# Local Operation Golden Session

This task freezes product-control behavior and helper-only operation behavior through real processes, platform endpoint, SQLite store, scheduler, waiters, and artifact store.

**Steps:**

1. Author complete product and internal test-daemon request/result/diagnostic fixtures from the architecture before running the product binary and the existing development binary's test-daemon subcommand.
2. Drive product hello, retained `daemon.ping`, daemon status, incompatible operation with control reuse, target/revision mismatch guidance, unavailable execute with empty list, exact-current-nonce `daemon.stop`, restart with a fresh nonce, and stale-prior-nonce refusal without replacement-side effects.
3. Drive helper execute, same-partition replay/conflict, same identifier in another terminal partition, bounded list/cursor, status, two waits, all conditional recovery evidence/category combinations, exact recovery resume plus replay after later/terminal state, every legal conditional terminal disposition including ResultUnavailable/AuthoritativeRemoteSuccess without a generic compensation-safety claim, inline/canonical results, deterministic artifacts, exact interrupted/resumed chunks, inline and associated complete maintenance preview, metadata lookup, unchanged and transfer-raced read start, supersession, durable apply/result replay, retirement, stop, restart, and replay.
4. Compare every response byte-for-byte, independently verify each maintenance metadata snapshot, allowed lookup-to-start transition, artifact or maintenance-result checksum/length/canonical bytes, and every nonzero-offset same-handle prefix before the first response byte, and prove each maximum complete maintenance/result document either fits its exact inline bound or is preserved completely through its declared target-qualified association and operation-free metadata-then-read path without hash inversion.
5. Capture diagnostics separately and assert startup/diagnostics never write into client protocol output; run twice against fresh roots with only opaque nonces normalized. Register each spawned daemon with the Plan 0001 stable-child supervisor, use current-nonce cooperative stop when responsive, retain the exact handle for unresponsive/induced-failure cleanup, and reap it before removing the root without process-identifier lookup or signalling.

**Tests:**

- Product and internal test-daemon sessions match committed output fixtures and documented process statuses, and both use only the two existing workspace binaries.
- Both waiters receive monotonic progress and one identical terminal result.
- The artifact verifies independently, every byte appears only in a bounded artifact-chunk frame, and interruption resumes from offset `1`, `length - 1`, and `length` only after exact same-handle prefix hash/discard; prefix mutation returns no `ArtifactStart` or chunk.
- Control remains available under operation incompatibility; identity/revision mismatch and unavailable execution create no row; a stale nonce cannot stop the replacement even if a diagnostic process identifier is reused.
- List cursors, recovery-resume receipts, and terminal maintenance manifests/application receipts/result associations remain bounded/target-partitioned; stop/restart preserves replay and metadata-readable associations without invoking the fake again, apply permits only the exact current-preview-to-application-receipt metadata/start transition, while supersession/retirement before read start makes only the exact former association unreadable.

- **Done when:** `cargo test -p slingshot-development --test local_operation_session` byte-matches both committed sessions and proves digest-bound retained control, no-row product execution, target-partitioned helper lifecycle/list, durable resume and maintenance replay, complete inline-or-operation-free-associated maintenance payloads through authenticated metadata/read, conditional terminal payloads, bounded inline-or-artifact results, exact same-handle prefix-authenticated resumable chunks, and isolated diagnostics, and all workspace gates succeed.
