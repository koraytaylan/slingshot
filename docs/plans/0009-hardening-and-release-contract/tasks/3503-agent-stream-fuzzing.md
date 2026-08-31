---
id: agent-stream-fuzzing
title: "Agent Stream Fuzzing"
workstream: "0035"
kind: task
depends_on:
  - configuration-parser-fuzzing
  - local-protocol-fuzzing
gated: false
touches:
  - fuzz/Cargo.toml
  - fuzz/fuzz_targets/agent_protocol_server_sent_event.rs
  - "fuzz/corpus/agent_protocol_server_sent_event/**"
  - crates/slingshot-development/tests/agent_stream_fuzz_corpus.rs
status: done
merged_as: ""
---
# Agent Stream Fuzzing

Author responses and Server-Sent Event streams cross the remote trust boundary. This task combines their bounded decoders with the pure job reducer while keeping network activity outside the target.

**Steps:**

1. Commit capability, submission, lookup, stream-generation, high-water, cursor-expired/reset, snapshot, event, successful result, failure, required artifact-reference, post-success interrupted acquisition, invalid artifact, expired result retention, heartbeat, multiline-data, installation-derived identifier, retry, untracked-event, duplicate-sequence, conflict, truncation, and size-bound seeds first. The capability/result corpus includes exact and independently drifted five-field command identities plus separate canonical-contract artifact/digest/dual-annotation provenance, canonical/noncanonical raw-byte and typed-order cases, exact `1.0.0+01` accepted and `1.0.0-01` rejected grammar vectors, canonical phrase/asset/token inputs, exact `AssetByteLength` zero/maximum/next/negative/fraction/exponent/nonminimal/overflow cases, every exact version-`1.0.0` continuation/configuration/content-package/add-component failure, and maximum inline plus first-externalized results.
2. Register one target that selects the relevant production decoder from an input discriminator and never opens a network connection.
3. Feed accepted job events, cursor resets, and snapshots through the reducer and subscription-level cursor fold, asserting sequence and watermark monotonicity, snapshot-ahead stale-event handling, duplicate equivalence, conflict refusal, rolling-window compaction, terminal immutability, and transport-derived post-success result/artifact recovery without accepting an agent-selected execution disposition.
4. Consume Plan 0003's independent fake-agent scenarios for: bounded opaque continuation framing and exact malformed/integrity/wrong-target/wrong-query/expired precedence; escaped `(service.pid=...)`, `listConfigurations`-only lookup, exact PID postcheck, factory-instance preservation, match/time bounds, exactly one `getProperties()` acquisition and one complete keys-only enumeration, and no create/bind/second-acquisition/second-enumeration call; all nine configuration scalar identities and four carriers including empty sequences, hostile Unicode/null/mixed/nested/type-disagreed values, case-folded duplicate keys, provider/designate/definition absence/ambiguity/failure, ordinary-versus-factory PID applicability, bundle-location scope, and metatype/name redaction before any value access followed by exactly one access per visible value; finite-set FileVault filter/manifest generation with Java-regex and XML adversaries, structural directory-only ancestors, profile negotiation, checked budgets, no widening/import, and publication disposition; and authoritative-no-effect `parent_not_orderable` before InFlight/mutation.
5. Assert line, event, body, collection, identifier, manifest-owned command, result, and artifact bounds before retained allocation. An authenticated result is still rejected before persistence or forwarding unless its submitted-command digest, exact five-field command identity, separately authenticated canonical-contract artifact/digest/dual annotations, raw canonical bytes/typed order, decoded Draft 2020-12 role shape, typed facts, exact closed result schema, request-derived facts, and artifact disposition match the durable operation in the required validation order.
6. Add the deterministic seed runner to ordinary automation and emit its canonical accepted fake-agent observations for the later protocol-compatibility parity matrix; do not make this earlier task consume a later snapshot task.

**Tests:**

- Every accepted protocol value round-trips canonically and every accepted stream dispatches only blank-line-terminated events.
- Comment heartbeats and allowed untracked events follow the contract without creating job transitions; an accepted untracked event can advance only the subscription cursor atomically.
- Duplicate subscription cursors and per-job event sequences agree or conflict deterministically, interleaved jobs remain independent, snapshot watermarks classify lower unseen events as stale, and no generated input changes a terminal state.
- Cursor expiry and generation changes require the high-water/snapshot/reset transition and cannot silently resubmit an operation whose idempotency history is unknown.
- Cross-origin or traversal-shaped artifact references are always rejected; a proven remote success followed by retryable result/artifact acquisition yields fieldless `RecoveryExecutionEvidence::AuthoritativeRemoteSuccess`, while irrecoverable required-result loss yields only `ResultUnavailable` with fieldless terminal `AuthoritativeRemoteSuccess`.
- Missing/stale/different command provenance, a result-schema role swap, submitted-command mismatch, noncanonical request echo, unknown/aliased failure, wrong reason/budget/path/index field, or a result above its registered bound is rejected before persistence or presentation.
- Configuration traces call `getProperties()` exactly once, enumerate its complete key set exactly once, and never call `getConfiguration`, `getFactoryConfiguration`, create, bind, or perform a second acquisition/enumeration; filter metacharacters cannot inject another clause; mismatched/ambiguous persistent identifiers and late/over-count lookups produce their exact no-partial failures. Redacted or rejected properties receive zero value reads and expose no carrier/value metadata, while each visible property receives exactly one value read and preserves original key case, exact scalar/carrier identity, order, signed width, and IEEE bits.
- FileVault fixtures prove literal `(`, `+`, `^`, `$`, backslash, `\E`, ampersand, both quote forms, both angle characters, supplementary Unicode, and invalid XML scalars never widen selection; only the supported `slingshot.filevault-merge-properties/1` profile records exact installed version, `merge_properties`, and `acHandling=ignore`; Slingshot never imports. A non-orderable component parent creates neither InFlight state nor repository/order effect.

- **Done when:** `cargo test -p slingshot-development --test agent_stream_fuzz_corpus` and `scripts/run_fuzz_target agent_protocol_server_sent_event` pass all retained inputs with bounded decoders, exact five-field identity plus separate canonical-contract pre-persistence provenance and independent role drift, raw-canonical-before-schema-before-typed enforcement, exact `+01`/`-01` and `AssetByteLength` vectors, the complete Plan 0003 fake-agent conformance inventory, lossless closed outcomes, reducer invariants, and transport-derived authoritative-remote-success recovery/result-unavailable evidence, and `scripts/quality` succeeds.
