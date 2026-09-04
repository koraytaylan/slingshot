---
id: standard-input-is-bounded-while-read
title: "Standard Input Is Bounded While Read"
workstream: "0062"
kind: task
depends_on: []
gated: false
touches:
  - crates/slingshot-command-line/src/command_line.rs
  - crates/slingshot-command-line/src/model_context_protocol/standard_stream_transport.rs
  - crates/slingshot-command-line/tests/model_context_protocol_transport.rs
  - crates/slingshot-command-line/tests/model_context_protocol_process_boundaries.rs
status: complete
merged_as: "pending"
---
# Standard Input Is Bounded While Read

The message parser has a line limit, but the product first calls unbounded `read_line`. A peer can withhold the newline and make the process allocate beyond the bound before the parser is invoked. The duplicate scanner also treats escape spellings as member identity.

**Steps:**

1. Read incrementally into a fixed bounded frame, stopping at limit plus one without allocating or consuming an unbounded remainder; define deterministic EOF, newline, invalid UTF-8, and recovery behavior.
2. Decode JSON member names during parsing and reject semantic duplicates, including Unicode escapes and surrogate pairs, before any map overwrites one value with another.
3. Enforce the nesting limit during deserialization rather than after a complete `Value` tree exists, and retain the closed request/notification direction rules.
4. Drive the compiled protocol process with exact-limit, plus-one, huge no-newline, huge whitespace, fragmented UTF-8, literal duplicate, escaped duplicate, and nested duplicate inputs; prove bounded refusal and no partial response.

- **Done when:** no standard-input message can allocate or read beyond the declared bounded lookahead, semantically duplicate members never reach dispatch, and exact-bound valid messages still round-trip through the compiled process.
