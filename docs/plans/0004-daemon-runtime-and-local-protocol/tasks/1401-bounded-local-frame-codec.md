---
id: bounded-local-frame-codec
title: "Bounded Local Frame Codec"
workstream: "0014"
kind: task
depends_on:
  - daemon-runtime-contract
gated: false
touches:
  - crates/slingshot-local-protocol/src/framing.rs
  - crates/slingshot-local-protocol/tests/framing.rs
  - "crates/slingshot-local-protocol/tests/fixtures/framing/**"
status: done
merged_as: "be2125c93296db88a88c169cc8a7aac3c3023b73"
---
# Bounded Local Frame Codec

The daemon and every local client need one allocation-safe frame boundary before either side interprets a message. This task implements the length-prefixed byte codec independently of operation semantics.

**Steps:**

1. Author raw-byte fixtures and `framing.rs` integration cases first for valid, fragmented, boundary-sized, zero-length, oversized, truncated, trailing, invalid UTF-8, partial-prefix, partial-payload, and byte-progress frames.
2. Consume the embedded typed `FoundationContract` for its fixed frame prefix, byte order, payload/nesting/collection bounds, and identifier/error bounds and the typed `DaemonRuntimeContract` only for Plan-0004-owned envelope/result limits; do not copy either manifest value into a constant, fixture default, or caller override. Extend asynchronous frame reading so it validates the declared length before allocating the payload and exposes zero/partial-prefix/partial-payload progress to the server. Server deadlines remain outside the codec and are read from their owning typed contract.
3. Implement frame writing with complete-write and flush behavior, mapping partial transport failures to structured framing errors without panicking.
4. Keep the codec unaware of JavaScript Object Notation fields, runtime namespaces, operations, and daemon state.

**Tests:**

- Every valid fixture round-trips byte-for-byte through independently fragmented reads and writes.
- Both repository manifests, embedded bytes, typed contracts, codec boundaries, and fixtures agree; replacing any bound with a Plan-0004 literal/default or assigning it to the wrong manifest fails the inventory assertion.
- A declared payload at the maximum succeeds; the first value beyond it fails before payload allocation.
- Zero-length, truncated-header, truncated-payload, invalid UTF-8, and trailing-byte cases produce distinct deterministic errors.
- One malformed frame cannot make the reader consume bytes belonging to a following connection.
- Every fragmentation point reports exact progress so server code can distinguish a quiescent complete-frame boundary from an incomplete frame without applying a clock inside the codec.

- **Done when:** `cargo test -p slingshot-local-protocol --test framing` passes every committed raw-byte fixture using only the embedded typed `support/foundation-contract.toml` values, including pre-allocation oversized refusal and exact incomplete-prefix/payload progress at all fragmentation points, and `cargo test --workspace --all-features`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, and `cargo fmt --all -- --check` succeed.
