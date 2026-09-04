---
id: subscription-events-are-generation-fenced
title: "Subscription Events Are Generation Fenced"
workstream: "0073"
kind: task
depends_on: ["success-and-result-settle-together"]
gated: false
touches:
  - crates/slingshot-storage/src/agent_subscription_ledger.rs
  - crates/slingshot-storage/migrations/**
  - crates/slingshot-storage/tests/agent_job_repository.rs
status: completed
merged_as: ""
---
# Subscription Events Are Generation Fenced

Event facts and rows omit generation while reset installation unconditionally overwrites the ledger. An old live stream can append after a reset, a late response can regress generation, and a reused cursor collides with the prior generation.

**Steps:**

1. Carry generation in every event fact, event primary identity, query, and persisted row; make counters and cursor high water explicitly generation-scoped.
2. Insert/advance events only with a transaction-local compare against the current generation and cursor. Refuse old or future generation input without changing bytes or counters.
3. Install reset high water with expected prior generation, incident, and cursor plus a monotonic new generation in one compare-and-set transaction; atomically archive or remove prior-generation events and reset their counters.
4. Race an old event with reset, two reset responses, and old/new streams reusing the same cursor; restart and compact after each winning order.

- **Done when:** old-generation writers cannot mutate the new ledger, reset cannot regress or overwrite a newer generation, cursor reuse across generations is unambiguous, and restart preserves generation-scoped events and counters.
