---
id: target-runtime-namespace
title: "Target Runtime Namespace"
workstream: "0016"
kind: task
depends_on:
  - daemon-runtime-contract
gated: false
touches:
  - crates/slingshot-daemon/src/runtime_namespace.rs
  - crates/slingshot-daemon/tests/runtime_namespace.rs
  - "crates/slingshot-daemon/tests/fixtures/runtime_namespace/**"
status: planned
merged_as: ""
---
# Target Runtime Namespace

Daemon ownership must be unique for one profile and environment without making unrelated targets contend or allowing user-controlled names to escape the runtime root.

**Steps:**

1. Author namespace vectors first for ordinary, Unicode, punctuation, maximum-length, path-like, case-distinct, and collision-attempt profile and environment names, including identical names paired with different author base addresses, selected-environment revisions, and credential fixtures.
2. Consume `FoundationContract` for profile/environment bounds, its exact namespace digest rule/encoding, Unix socket-address bound, Windows named-pipe-name bound, and readiness-record bound. Derive a namespace key only from the canonical profile and environment names, readable escaped components, and that manifest-defined digest; do not restate those values or introduce a fallback hash/name limit.
3. Resolve typed platform endpoint identifiers, owner-lock identifiers, and readiness paths beneath an injected ephemeral per-user runtime root, and resolve target database, artifact, global installation, registered-target, maintenance, and diagnostic paths beneath an independently injected persistent per-user state root, without following user-controlled path components. Endpoint creation remains in the Plan 0001 platform adapter/daemon-server task; Windows endpoint identity is not a filesystem path.
4. Create both namespace directories with current-user access and validate existing directory ownership and type before use.

**Tests:**

- Every vector maps deterministically to its committed namespace key.
- Distinct profile/environment pairs remain distinct, including names that sanitize to the same readable prefix.
- Changing author base address, selected-environment revision, same-principal secrets, or the opaque authentication principal for one pair leaves its namespace key unchanged. Same-principal rotation preserves the target digest, while a principal change alters that digest and partitions durable operation/artifact data under the same process namespace.
- Path separators, parent markers, absolute paths, and input one unit beyond each manifest boundary cannot escape or overflow either root; exact manifest boundaries succeed.
- Two target namespaces can be created and used concurrently without sharing any path.
- Replacing the ephemeral runtime root simulates a new login while the database and artifacts remain available through the persistent state root.

- **Done when:** `cargo test -p slingshot-daemon --test runtime_namespace` matches every vector, proves name-only process ownership plus target-partitioned durable paths, dual-root containment and collision resistance, and state preservation across runtime-root replacement without touching real user directories, and all workspace gates succeed.
