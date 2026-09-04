---
id: one-frame-reader-lives-with-the-connection
title: "One Frame Reader Lives With The Connection"
workstream: "0063"
kind: task
depends_on: []
gated: false
touches:
  - crates/slingshot-daemon/src/local_server.rs
  - crates/slingshot-daemon/tests/ping_service.rs
status: complete
merged_as: "pending"
---
# One Frame Reader Lives With The Connection

One socket read may return several frames. The server's per-call buffer returns the first payload and drops every already-read trailing byte, so correctness depends on clients waiting between writes and on kernel packetization.

**Steps:**

1. Retain one `FrameReader` for the entire accepted connection and consume every complete buffered frame before awaiting more bytes.
2. Preserve first-frame, partial-frame, absolute, shutdown, size, and canonical-frame deadlines while distinguishing buffered progress from transport progress.
3. Send two and three concatenated frames in one write, a fragmented first frame followed by a complete second, multiple frames followed by EOF, and mixtures around exact bounds.
4. Require one ordered response per request with no loss, duplication, extra read, or wait for bytes already buffered.

- **Done when:** response count and order depend only on the framed byte stream, not how the operating system partitions reads, for every concatenated and fragmented fixture.
