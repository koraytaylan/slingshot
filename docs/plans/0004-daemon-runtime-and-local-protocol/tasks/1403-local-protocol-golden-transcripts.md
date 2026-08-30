---
id: local-protocol-golden-transcripts
title: "Local Protocol Golden Transcripts"
workstream: "0014"
kind: task
depends_on:
  - local-operation-envelopes
  - stable-local-control-protocol
gated: false
touches:
  - crates/slingshot-local-protocol/tests/transcripts.rs
  - "crates/slingshot-local-protocol/tests/fixtures/transcripts/**"
status: done
merged_as: ""
---
# Local Protocol Golden Transcripts

The frame codec and envelopes compose into one observable stream contract. This task pins that composition with hand-authored bytes before a daemon dispatches them.

**Steps:**

1. Write independent input and output transcript fixtures for retained control under compatible and incompatible operation versions and `DaemonRuntimeContractDigest` values, exact-current and stale-nonce `daemon.stop`, every operation request including durable exact recovery-resume replay after later/terminal state, inline and external complete maintenance preview, applied/replayed receipt, target-qualified `MaintenanceResultMetadata` followed by reads at offsets zero/one/length-minus-one/length, unchanged metadata, the sole current-preview-to-application-receipt owner/revision transition, superseded/retired/mismatch refusal, bounded artifact chunks, same-author changed-principal target mismatch, revision mismatch, raw-principal-field rejection, progress fan-out, every legal conditional terminal result, rejected illegal terminal combinations, structured failures, and connection close. Derive inherited boundaries from the typed `FoundationContract` and Plan-0004 boundaries from the typed `DaemonRuntimeContract`; fixtures may encode a boundary case but cannot introduce a second named/default value.
2. Build an in-memory transcript driver that feeds every input byte boundary through the production frame reader and records every frame writer byte.
3. Compare decoded message sequences and the entire encoded output stream byte-for-byte, including more than one progress response for one wait request.
4. Assert that stored bytes occur only in a bounded `ArtifactChunk` or `MaintenanceResultChunk` response, maintenance-result metadata contains exactly its nine association facts and no bytes/path/operation/artifact/slot, the metadata request is keyed only by target and identifier, every operation fixture is versioned, and control fixtures remain decodable without an operation version.

**Tests:**

- A complete compatible session matches its output transcript byte-for-byte under every read fragmentation boundary.
- An incompatible operation request produces one refusal while the same connection still completes daemon-status and exact-current-nonce `daemon.stop`; a prior-instance nonce returns `stale_daemon_instance` with no replacement-side transition.
- Expected-target, including an opaque-principal difference, and selected-environment-revision mismatches produce their exact guidance response and consume no repository request; transcripts carry no raw principal tuple.
- Malformed, oversized, and truncated inputs end only their transcript and never panic.
- Canonical output is identical across repeated runs and independent of map insertion order.

- **Done when:** `cargo test -p slingshot-local-protocol --test transcripts` byte-matches every committed transcript under all fragmentation boundaries and proves retained control under version/digest incompatibility, target/revision refusal, durable recovery-resume and maintenance-apply replay, complete inline-or-associated maintenance payloads and operation-free metadata-then-read including the permitted ownership race, conditional terminal payloads, progress, and bounded artifact sessions, and all workspace gates succeed.
