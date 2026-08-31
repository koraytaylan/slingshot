---
id: local-request-dispatch
title: "Local Request Dispatch"
workstream: "0017"
kind: task
depends_on:
  - artifact-chunk-service
  - operation-wait-and-progress
  - list-operations
  - resume-operation-recovery
  - terminal-operation-maintenance
  - daemon-local-server
gated: false
touches:
  - crates/slingshot-daemon/src/request_dispatch.rs
  - crates/slingshot-daemon/src/local_server.rs
  - crates/slingshot-daemon/tests/request_dispatch.rs
status: done
merged_as: ""
---
# Local Request Dispatch

The local server becomes useful only through one exhaustive mapping from negotiated envelopes to operation services and stable response codes.

**Steps:**

1. Author table-driven dispatch tests first for every retained control and versioned operation request plus every target/revision, domain, storage, capacity, executor, operation artifact, operation-free maintenance-result metadata/read association, waiter, recovery-resume receipt, complete maintenance manifest/application receipt, and internal failure class.
2. Dispatch hello, retained `daemon.ping`, daemon status, and retained `daemon.stop` through the stable control path even when operation versions do not intersect. Stop reaches shutdown only after exact-current-nonce validation; preserve `stale_daemon_instance` with no side effects for a prior-instance nonce.
3. Before any operation service access, require a supported operation version, exact expected `DaemonRuntimeContractDigest`, and exact expected `AuthorTargetIdentity` plus `SelectedEnvironmentRevision`; return explicit update or stop/restart guidance on mismatch without routing or stopping.
4. Route execute, operation status, bounded list, wait, result, durable exact-precondition recovery resume, operation-artifact read, target-qualified maintenance-result metadata, target-qualified maintenance-result read, and complete terminal maintenance preview/apply through owning local services without duplicating validation, persistence, command-line parsing, or rendering.
5. Map each typed failure to one stable bounded local code and redacted diagnostic.
6. Keep both control and operation matches exhaustive so a new variant fails compilation until dispatch and tests name it.

**Tests:**

- Every request reaches exactly its intended service and returns the expected response variant; control remains usable under operation incompatibility.
- Every known failure class maps to its committed stable code and does not expose internal paths or sensitive fixture values.
- Requests before hello and operation requests with another namespace, target, or revision are refused without repository/executor access and never trigger automatic stop; a stale stop nonce cannot reach shutdown or affect a replacement.
- A failing request leaves the connection usable for a following well-formed request unless framing or version compatibility requires close.

- **Done when:** `cargo test -p slingshot-daemon --test request_dispatch` exhausts stable-control and versioned-operation routing including durable recovery-resume, maintenance-application replay, and operation-free maintenance-result metadata/read/no-start refusal, proves pre-service identity/revision gates plus mismatch guidance and redaction, and leaves control healthy after incompatibility or recoverable errors, and all workspace gates succeed.
