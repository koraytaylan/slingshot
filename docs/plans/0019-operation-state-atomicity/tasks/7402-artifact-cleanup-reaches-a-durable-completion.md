---
id: artifact-cleanup-reaches-a-durable-completion
title: "Artifact Cleanup Reaches A Durable Completion"
workstream: "0074"
kind: task
depends_on: ["maintenance-application-revalidates-in-its-transaction"]
gated: false
touches:
  - crates/slingshot-storage/src/maintenance.rs
  - crates/slingshot-storage/src/artifact_store.rs
  - crates/slingshot-storage/migrations/**
  - crates/slingshot-daemon/tests/operation_maintenance.rs
status: completed
merged_as: ""
---
# Artifact Cleanup Reaches A Durable Completion

Database maintenance can make a blob unreferenced, but the physical release function has no caller and no receipt ever advances to `Completed`. Bytes can therefore remain forever outside the result the receipt reports.

**Steps:**

1. Persist one deletion-work item for every artifact candidate in the database phase, bound to digest, verified store identity, receipt, and expected unreferenced state.
2. Process work idempotently after commit: recheck references, open the digest object through the hardened store boundary, delete and synchronize it, then remove its blob/accounting rows and advance the receipt only when absence is durable.
3. On a new reference or uncertain filesystem result, retain/retry conservatively; never delete a referenced, substituted, or digest-mismatched file.
4. Crash after database apply, before unlink, after unlink, and before completion; restart each state and prove eventual complete cleanup, byte-identical receipt replay, and no leak or unsafe deletion.

- **Done when:** every unreferenced physical artifact selected by maintenance is durably removed and its receipt reaches `Completed` across retries/restarts, while referenced or identity-mismatched bytes are always retained and reported pending or refused.
