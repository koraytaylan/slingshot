---
id: pin-command-golden-sessions
title: "Pin Command Golden Sessions"
workstream: "0028"
kind: task
depends_on:
  - build-command-process-harness
gated: false
touches:
  - crates/slingshot-command-line/tests/command_golden_sessions.rs
  - crates/slingshot-test-support/fixtures/command-golden-sessions/**
status: done
merged_as: ""
---
# Pin Command Golden Sessions

Pin process-level standard streams, exits, daemon bootstrap/reuse, lost-receipt recovery, and phase-specific signals for every catalog and observation command in human and JSON modes.

**Steps:**

1. Record readable scenario sources and expected byte files for the existing complete command/observation surface; exact `slingshot.daemon-runtime-contract/1` and `slingshot.author-agent-transport-contract/1` bytes/sidecar digests; the exact `slingshot.command-canonical-json/1` artifact/digest, command-schema-manifest value, and both role roots' `x-slingshot-canonical-json-contract-sha256` annotations; independent exact wire-name/`1.0.0`/limits/argument-schema/result-schema acceptance and drift refusal; raw-byte/Draft-2020-12/typed ordering; inline and operation-free target-qualified associated maintenance metadata/read/preview/apply/result values through replay/restart/ownership transfer/supersession/retirement; untrimmed SearchPhrase; canonical ascending asset format/tag sets; exact-bound opaque continuation; exact closed Plan 0002 configuration-check diagnostics including the inclusive 32-item marker; and every revised continuation/configuration/FileVault/add-component failure object in human and JSON modes, including all exact reason/budget/path fields and outcome-unknown dispositions.
2. Use the scaffolded test-only `slingshot-test-support` dependency, compose its retained-instance process harness and Plan 0004 current-nonce cooperative cleanup adapter with scripted local-protocol endpoints, and run each scenario in terminal and redirected modes. Use only the retained child/native handle or supervision channel for an unresponsive owned child; record process identifiers only as diagnostics.
3. Add compiled-process scenarios for an absent namespace, reuse of an existing compatible owner, barrier-released concurrent auto-start, early child exit, readiness timeout/failure, target/revision mismatch, detached and attached reuse, and post-admission connection loss before receipt followed by a fresh process with the exact same retry identifier. Assert one daemon owner, one durable operation/executor invocation, exact receipt replay, and no identifier replacement.
4. Deliver real process signals before receipt validation, after accepted/replayed receipt during wait and result acquisition, and independently immediately before/at/after atomic operation-artifact publication, immediately before/at/after operation-free maintenance-result publication, and before/during/after final output commit. Pin exact pre-publication human/JSON interruption, exit `130`, empty-versus-single-envelope stdout rules, retained retry/durable or target/result identifiers, zero remote cancellation, and no newly published destination. Pin either publication as exit-`0` success against at/post-publication signals; force process/output loss after publication and prove an identical fresh invocation authenticates the retained receipt and re-renders success without download, publication, or collision.
5. Exercise responsive current-nonce daemon stop, stale-nonce replacement refusal, unresponsive owned-child cleanup through the retained instance handle/channel, and process-identifier reuse beside an unowned sentinel. Compare standard output, standard error, exit, daemon transcript, child ownership/cleanup, created artifacts, and every signal/lost-receipt race exactly, with an explicit review command for fixture updates.

**Tests:**

- `command_golden_sessions` covers every command metadata leaf, absent/reused/concurrent startup, early-exit/readiness failure, target/revision mismatch, post-admission lost-receipt replay, all four interruption variants, independent operation-artifact/maintenance-result-publication/rendering boundaries, receipt-backed success re-rendering, safe cleanup, and both output modes against the compiled executable.
- A coverage assertion fails if any registered operation or exit class has no golden session.
- Coverage also fails if any version-`1.0.0` semantic failure category/reason/budget lacks both lossless machine and exact-literal human sessions; any daemon-runtime/author-agent-transport/canonical-contract artifact/manifest/annotation relation is unauthenticated; raw-byte, standard-schema, and typed rejection order is not observed; an over-inline maintenance value is truncated, independently serialized, operation-shaped, or loses its exact URI/metadata/read lifecycle; a configuration diagnostic exposes a forbidden provenance/name/path/order/suggestion field; or a stale limits digest, trimmed phrase, noncanonical asset set, or decoded/rewritten token is accepted.
- Provenance coverage includes one-at-a-time runtime-digest and transport-digest drift, contract-only drift with fixed five-field identity and fixed role bytes/digests, plus one-at-a-time limits/argument/result drift. Process coverage fails if any of the five identity fields lacks an independent drift case, any claimed startup/lost-receipt/signal phase is absent, a pre-receipt branch claims durable admission, a post-receipt branch loses receipt facts, either pre-publication transfer interrupt exposes a path or publishes a destination, maintenance-result interruption carries an operation/slot, an at/post-publication signal yields interruption/resume/collision, or cleanup signals by diagnostic process identifier or harms a replacement.

- **Done when:** `cargo test -p slingshot-command-line --test command_golden_sessions` matches byte-exact compiled-process sessions for authenticated daemon-runtime/author-agent-transport/canonical-contract/annotation/five-field provenance, raw-byte/schema/typed ordering, inline/operation-free-associated complete maintenance metadata/read parity and lifecycle, exact closed configuration diagnostics, complete revised semantic-failure inventory, absent/reused/concurrent daemon startup, safe retained-instance cleanup, one-operation lost-receipt replay, phase-specific interruption plus both publication-success receipt recoveries, command/exit catalogs, recovery replay, and terminal evidence.
