---
id: success-and-result-settle-together
title: "Success And Result Settle Together"
workstream: "0072"
kind: task
depends_on: ["recovery-receipts-belong-to-one-operation"]
gated: false
touches:
  - crates/slingshot-domain/src/operation.rs
  - crates/slingshot-daemon/src/operation_submission.rs
  - crates/slingshot-daemon/src/operation_queries.rs
  - crates/slingshot-storage/src/operation_repository.rs
  - crates/slingshot-storage/migrations/**
  - crates/slingshot-development/tests/operation_submission_process.rs
  - crates/slingshot-daemon/tests/operation_queries.rs
status: completed
merged_as: ""
---
# Success And Result Settle Together

The success lifecycle commits before a second transaction writes a disposition label. Actual inline bytes and complete artifact associations are not stored by this path, and outstanding recovery is retained. A crash can leave a permanent succeeded row whose result query can only report missing disposition.

**Steps:**

1. Define one settlement value carrying canonical inline bytes or verified artifact associations, immutable disposition, settled time, expected prior lifecycle/version, and recovery resolution.
2. Commit lifecycle success, full result, associations/capacity conversion, disposition, time, and recovery clearing in one compare-and-set transaction. Remove or make private the standalone disposition mutator.
3. Add schema checks or triggers for valid lifecycle/result/disposition/recovery combinations and a migration that detects rather than silently blesses existing torn state.
4. Inject failure before every statement and commit, race two settlements, settle a recovered operation, and reopen; require either the complete prior recoverable state or byte-identical complete success.

- **Done when:** a succeeded operation always has its complete immutable readable result and no outstanding recovery in the same commit, no nonsucceeded row can acquire a result disposition, and fault/restart tests never produce a torn query state.
