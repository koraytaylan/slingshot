---
id: schedulers-claim-before-execution
title: "Schedulers Claim Before Execution"
workstream: "0075"
kind: task
depends_on: ["the-daemon-serves-versioned-operations"]
gated: false
touches:
  - crates/slingshot-daemon/src/operation_scheduler.rs
  - crates/slingshot-storage/src/operation_repository.rs
  - crates/slingshot-storage/migrations/**
  - crates/slingshot-daemon/tests/operation_scheduler.rs
status: planned
merged_as: ""
---
# Schedulers Claim Before Execution

The scheduler chooses eligible rows from a snapshot but commits no claim. If two product ticks use that selector, both can start the same operation before either lifecycle update becomes visible.

**Steps:**

1. Add a transactionally leased and fenced claim containing operation, expected lifecycle/version, claimant generation, and bounded expiry; selection plus claim is one repository operation.
2. Permit only the live winning fence to call the executor, renew, record progress, or settle, and make stale workers unable to mutate the operation after lease transfer.
3. Recover an expired claim from durable submission/transport evidence and Plan 0019 state, choosing resume, uncertainty, or terminal refusal without authorizing a second logical effect.
4. Overlap ticks in threads and processes, crash after claim and remote send, expire/renew/transfer leases, cancel, and restart; assert at most one live executor and one accepted settlement fence.

- **Done when:** concurrent schedulers can never execute or settle one logical operation under two live claims, and every crash/expiry/restart resumes from one durable fence without losing or inventing remote-effect evidence.
