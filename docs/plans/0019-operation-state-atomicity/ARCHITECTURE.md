# Plan 0019 — Operation State Atomicity

## Architectural boundary

Every state transition exposes one repository method whose transaction reads the exact current preconditions, compares an expected version or generation, applies all dependent rows/counters, and writes an immutable receipt. Callers may prepare canonical inputs outside the transaction; they may not make a durable eligibility decision there.

Recovery receipt identity contains the target, operation, source fingerprint, and category. Successful settlement stores the actual canonical result representation, not a label describing what was returned transiently. Schema constraints make impossible lifecycle/result/recovery combinations unrepresentable even for future callers.

Subscription generations fence both events and reset responses. Generation participates in row identity and counters; old-generation writers cannot pass a current-generation CAS. Byte admission computes existing plus incoming with checked arithmetic in the same transaction that inserts the event, and replay charges nothing.

Maintenance is a two-phase durable workflow. The database phase atomically revalidates reviewed rows, records exact effects and an immutable receipt, and creates deletion work. A retryable cleanup phase verifies reference state and filesystem identity before deletion, then marks the receipt completed. Replays return the same persisted result at every phase.

## What proves what

Concurrency tests place barriers between every former read/commit gap and assert one consistent winner. Fault injection stops before and after every statement/commit; reopen returns either the complete prior state or complete new state, never a defensive-query error. Tests use distinct operations with identical commands, old and new subscription generations with reused cursors, and exact/plus-one byte edges.

Maintenance tests change a reviewed row after preview, race two applies, crash after database deletion and before file deletion, and retain a concurrent reference. The receipt is byte-identical before and after restart and reaches completed only after unreferenced bytes are gone.

## What stays outside

The remote system can still return uncertain disposition; this plan preserves that evidence atomically rather than guessing its outcome.
