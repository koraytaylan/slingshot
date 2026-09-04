# Plan 0016 — Local Boundary Robustness

> Bound bytes before allocating them, preserve complete messages independent of packetization, apply real backpressure, and carry one unambiguous identity through every local request.

## Why this plan

The local protocol layers contain good policy models, but several are not on the production I/O path. Standard-input lines are accumulated without a limit and written synchronously while an unused queue models bounded output. The JSON duplicate scanner compares escape spellings rather than decoded member names. The daemon drops coalesced frames and lets post-response idle connections retain every connection permit forever. File-backed mutation input promises pre-allocation bounds and duplicate refusal but first materializes a generic JSON value. CLI retry identifiers are millisecond timestamps generated repeatedly during one request. Multiline private-key redaction stops at the newline after the opening marker.

All are boundary-contract defects: behavior changes with allocation pressure, JSON spelling, socket chunking, peer idleness, clock granularity, or line layout rather than with the logical request.

## In scope

- **0062 — Standard-stream transport.** Incremental bounded framing, semantic duplicate-member refusal, streaming depth enforcement, and one bounded deadline-aware output path whose acknowledgement follows a successful write.
- **0063 — Local daemon connections.** One retained frame reader preserves trailing frames, while idle post-response peers cannot exhaust the general connection pool; explicit long-lived waits have their own bounded lifecycle.
- **0064 — Mutation command boundaries.** Property documents are byte/member/depth bounded and duplicate-free before building a request, and `create_page.title` uses the same typed bound its public schema advertises.
- **0065 — Request identity.** One collision-resistant identifier is generated once per invocation and threaded through submission, observation, interruption, and retry advice.
- **0066 — Diagnostic confidentiality.** Complete PEM blocks and bearer credentials are removed across line boundaries, with adversarial fixtures over real multiline forms.

## Out of scope

This plan does not connect the Model Context Protocol catalog or daemon operation executor; Plan 0020 owns product composition after these transports are safe. It does not authenticate other users to a same-user endpoint, redesign JSON-RPC, or promise redaction of arbitrary application content. Network author transport is also outside this local boundary.

## Review evidence at `2808354`

- `command_line.rs::serve_protocol` calls unbounded `BufRead::read_line`; `read_message` checks the one-megabyte limit only after the `String` has grown. `MemberScanner` removes backslashes without decoding Unicode escapes, so `"id"` and `"\\u0069d"` evade duplicate detection before `serde_json::Value` silently chooses one. `ServerApplication::requested` ignores enqueue failure and acknowledges while returning the same line for direct synchronous output; the modeled `OutputQueue` and deadlines have no production consumer.
- `local_server.rs::read_frame` creates a fresh buffer for every frame and returns only the first payload, dropping trailing bytes already read. After a response, `deadline_for(FrameProgress::Empty, false)` returns no deadline while one semaphore permit is retained for the entire connection. Sixty-four valid ping-then-idle same-user peers can therefore block every new client indefinitely.
- `property_document.rs` states that bounds and duplicates are checked first, but `read_to_string` is unbounded and `serde_json::from_str::<Value>` collapses duplicates before a post-allocation depth walk. `CreatePageCommand.title` is a plain `String`; schema construction gives it a 65,536-byte property-string bound while the existing `PageTitle` value object and other title schemas use the 1,024-byte page-title bound.
- `application.rs::request_identifier` is `command-line-<milliseconds-since-epoch>` and is called separately for retry phases, request envelopes, and generated operation keys. Concurrent processes or requests in one millisecond collide, and an interrupt can quote a different identifier from the operation actually sent.
- `diagnostics.rs::redact_blocks` replaces from a PEM opening only to the next whitespace. For a normal PEM, that is the newline immediately after the opening, leaving the base64 body and end marker. The committed fixture places the entire key on one whitespace-free word and cannot expose the leak.
