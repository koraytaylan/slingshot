---
id: minimal-local-protocol
title: "Minimal Local Protocol"
workstream: "0003"
kind: task
depends_on:
  - workspace-module-map
  - workspace-capability-probes
gated: false
touches:
  - support/foundation-contract.toml
  - crates/slingshot-local-protocol/src/lib.rs
  - crates/slingshot-local-protocol/src/foundation_contract.rs
  - crates/slingshot-local-protocol/src/envelope.rs
  - crates/slingshot-local-protocol/src/framing.rs
  - crates/slingshot-local-protocol/src/ping.rs
  - crates/slingshot-local-protocol/tests/minimal_protocol.rs
  - "crates/slingshot-local-protocol/tests/fixtures/minimal-protocol/**"
status: done
merged_as: "014b50bedff1050810c09c99c4d9213593d3fc67"
---
# Minimal Local Protocol

The walking skeleton freezes one bounded retained-control request path and the sole canonical machine-readable source for every Plan 0001 wire, namespace-security, endpoint-interoperability, startup, and process-harness limit. Later operation protocols coexist with this retained control surface without replacing its framing, ping, nonce-bound stop, or error rules.

**Steps:**

1. Hand-author canonical ping/stop request, success, stale-stop-nonce, version-mismatch, malformed, duplicate-field, trailing-data, limit-exceeded, empty-prefix, partial-length, partial-payload, and fragmented-progress frames before defining serialization types.
2. Commit closed `support/foundation-contract.toml` with format `slingshot.foundation-contract/1` and exactly these values: retained-control version `1`; frame-length prefix `4` bytes; maximum frame payload `1_048_576` bytes; JavaScript Object Notation nesting `64`; collection items `1_024`; request-identifier UTF-8 `128` bytes; method UTF-8 `32` bytes; error-code UTF-8 `64` bytes; error-message UTF-8 `4_096` bytes; profile and environment UTF-8 `128` bytes each; SHA-256 namespace digest `32` bytes rendered as `64` lowercase hexadecimal bytes; readiness nonce `32` random bytes rendered as `64` lowercase hexadecimal bytes; readiness record `4_096` bytes; Unix socket address `100` bytes; Windows named-pipe name `240` UTF-16 code units; required Windows pipe flag `PIPE_REJECT_REMOTE_CLIENTS`; server connection capacity `64`; initial-control-frame `5_000` milliseconds; incomplete-frame read-idle `2_000` milliseconds; absolute frame completion `10_000` milliseconds; response write `5_000` milliseconds; explicit-start total `30_000` milliseconds; start retry maximum delay `100` milliseconds; cooperative stop `5_000` milliseconds; supervised termination-and-wait `10_000` milliseconds; process-test scheduling tolerance `5_000` milliseconds; and walking start-client count `20`. The schema rejects missing, additional, duplicate, negative, zero where positive is required, overflowed, differently encoded, or placeholder fields.
3. Implement one typed `FoundationContract` parser over the checked-in bytes embedded into `slingshot-local-protocol`; product/runtime/test code receives values only through this API. Tests parse the repository file and byte-compare it with the embedded input. No source module, fixture, script, platform row, or later plan may carry a second numeric value for these Plan 0001 limits; Plan 0003 remains owner of command-leaf and command-contract limits.
4. Implement the stable control request and response envelopes, caller request identifier, structured error envelope, `daemon.ping`, and `daemon.stop` with full-word Rust identifiers and stable serialized field names. Ping returns product version, process identifier as non-authoritative diagnostics, the current readiness nonce, and the currently empty supported operation-protocol version set. Stop requires the exact current readiness nonce, acknowledges before orderly shutdown, and returns `stale_daemon_instance` without side effects for a nonce from any prior instance.
5. Implement framing as the manifest's fixed-width unsigned payload length in network byte order followed by exactly one value, with its exact byte/nesting/collection bounds and refusal of non-whitespace trailing bytes. Framing reports whether zero bytes, a partial prefix, or a partial payload is pending so the server applies manifest deadlines without parsing twice. Keep parsing/rendering pure over byte slices; socket and clock code remain absent from this crate.

**Tests:**

- Every hand-authored accepted fixture decodes and re-encodes byte-for-byte canonically.
- Unknown control versions return the retained-control compatibility error without attempting method dispatch; compatible ping returns the current nonce/version set, and stop accepts only that nonce.
- Duplicate fields, truncated and overflowing length prefixes, invalid Unicode, trailing values, excessive nesting, oversized collections, and a frame one byte beyond the limit each fail with distinct bounded errors.
- The checked repository manifest, embedded bytes, typed values, fixtures, service deadlines, endpoint builders, startup loops, and process harness have one exact value for every listed field; deletion, duplication, source-code literal drift, or an additional plan-owned limit fails the inventory test.
- A frame, name, readiness record, cohort, and deadline exactly at each declared limit succeeds; the adjacent over-limit fixture fails before proportional allocation or side effects.
- Arbitrary malformed bytes never panic and never allocate beyond the frame limit.
- Fragmentation fixtures distinguish no pending frame from partial prefix/payload progress without using a process clock.
- A stale stop nonce cannot stop a replacement daemon even when its process identifier equals a prior diagnostic value.

- **Done when:** `cargo test -p slingshot-local-protocol --test minimal_protocol` passes every canonical, malformed, stale-nonce, and adjacent-boundary fixture, proves all Plan 0001 wire/security/interoperability/process limits come only from embedded `support/foundation-contract.toml`, and asserts the exact structured error for each refusal.
