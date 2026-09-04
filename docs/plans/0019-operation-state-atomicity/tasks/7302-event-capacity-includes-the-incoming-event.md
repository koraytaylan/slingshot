---
id: event-capacity-includes-the-incoming-event
title: "Event Capacity Includes The Incoming Event"
workstream: "0073"
kind: task
depends_on: ["subscription-events-are-generation-fenced"]
gated: false
touches:
  - crates/slingshot-storage/src/agent_subscription_ledger.rs
  - crates/slingshot-storage/tests/agent_job_repository.rs
  - policy/daemon-runtime-contract-1.json
  - policy/daemon-runtime-contract-1.sha256
status: planned
merged_as: ""
---
# Event Capacity Includes The Incoming Event

Admission refuses only when existing event bytes are already at the aggregate limit, then adds arbitrary incoming bytes. One first event or the last event near capacity can cross the entire bound.

**Steps:**

1. Declare and enforce a per-event byte maximum and compute existing plus incoming with checked arithmetic inside the same write transaction as insert.
2. Accept only when wanted bytes fit the generation-scoped remaining aggregate capacity; make exact replay idempotent and charge zero additional bytes.
3. Reconcile stored byte/count counters to the generation's event rows on open and after compaction, refusing overflow or unexplained drift.
4. Test exact bound, plus one, limit-minus-one crossing, oversized first event, integer overflow, concurrent contenders, replay, compaction, and restart.

- **Done when:** no committed event or concurrent subset exceeds its per-event or generation aggregate byte bound, replay charges zero, and persisted counters always equal the authoritative event rows after reopen.
