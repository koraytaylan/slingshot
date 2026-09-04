# Plan 0016 — Local Boundary Robustness

## Architectural boundary

Each ingress owns a reader that enforces the byte limit while bytes arrive, not after a convenience API allocates them. JSON validation decodes member names and tracks depth during deserialization into typed structures. Packet transport owns framing state for the lifetime of the connection.

Each egress has exactly one producer-consumer path. Admission reserves bounded output capacity, the writer consumes it under the declared deadline, and only the successful sink write acknowledges the request. A blocked sink causes deterministic detachment and shutdown without unbounded memory or a hung child.

Request identity is invocation state. It is created by an injectable collision-resistant generator once, then borrowed everywhere; time can be metadata but is not uniqueness. Redaction is a total transformation applied before truncation or persistence and recognizes a block through its matching terminator across lines.

## What proves what

Limit tests feed data incrementally without a newline and observe bounded memory/read-ahead at exactly the limit and one byte beyond. Duplicate tests use semantically identical keys with literal, escaped, and surrogate spellings. Output tests stop draining a pipe and prove capacity and deadline behavior at the real process boundary.

Socket tests coalesce and fragment multiple frames deliberately. Capacity tests fill the real endpoint with valid then idle clients and prove a later legitimate request succeeds after the lease while an explicit wait retains only its separately budgeted resource.

Identity tests freeze the clock and run concurrent invocations, then interrupt at every handoff. The retry identifier must byte-match the durable operation key. Redaction tests use real multiline PKCS#8 and RSA shapes with secrets before and after the block to prove scope is neither short nor overbroad.

## What stays outside

Transport safety establishes a usable boundary; it does not make a stub application useful. Product adapters arrive only in Plan 0020.
