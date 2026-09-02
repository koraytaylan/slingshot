---
id: single-daemon-ownership
title: "Single Daemon Ownership"
workstream: "0016"
kind: task
depends_on:
  - stable-local-control-protocol
  - target-runtime-namespace
gated: false
touches:
  - crates/slingshot-daemon/src/ownership.rs
  - crates/slingshot-daemon/src/platform_runtime/readiness.rs
  - crates/slingshot-daemon/tests/ownership.rs
  - crates/slingshot-daemon/tests/runtime_ownership.rs
  - crates/slingshot-development/tests/platform_runtime_contract.rs
status: done
merged_as: "558d9a6ee57c28cab89b24f5dc4d0b99ce02b25c"
---
# Single Daemon Ownership

The namespace needs one live owner and safe stale recovery. Process identifiers are untrusted diagnostics only and cannot establish either property or authorize cleanup.

**Steps:**

1. Write multiprocess tests first for first ownership, live contention, stale endpoint, stale metadata, reused process identifier, stale/prior-instance nonce, owner death, replacement, distinct namespaces, and same-namespace readiness whose author address matches but opaque authentication principal differs.
2. Reuse the Plan 0001 platform-equivalent namespace owner lock held for the daemon lifetime and publish atomic readiness metadata bounded by `FoundationContract`, containing namespace, its manifest-shaped nonce, process identifier, retained control version, supported operation versions, selected `AuthorTargetIdentity`, and `SelectedEnvironmentRevision`. Treat the process identifier solely as an output-correlation field: no code may look it up, check it, signal it, or use it for liveness/authority.
3. Probe the endpoint and exact hello nonce before classifying an apparent owner as live; equality of diagnostic process identifiers contributes no evidence.
4. Permit endpoint and metadata cleanup only while holding the owner lock and only when no matching live hello answers.
5. Remove shutdown artifacts only when their stored nonce still belongs to the current owner. A stale nonce cannot stop, clean, or mutate a replacement even if the operating system reuses the same diagnostic process identifier.

**Tests:**

- Exactly one process acquires one namespace and every live contender receives the current owner's metadata.
- A reused process identifier is never accepted as ownership, liveness, stop, or cleanup proof; only the lock plus exact live endpoint/nonce protocol establishes the current owner.
- One successor recovers stale endpoint and metadata after owner death; contenders do not delete a live endpoint.
- Different namespaces acquire ownership concurrently.
- Readiness and hello expose identical principal-bound author target and selected-environment revision without raw principal fields, and a stale record with another principal/revision is never treated as a live matching owner.

- **Done when:** `cargo test -p slingshot-daemon --test ownership` passes deterministic ownership/stale-recovery policy fixtures for every abstract platform row and the one matching current-native endpoint when present, including replacement, process-identifier reuse, stale-nonce refusal, and identity-bearing atomic readiness, while making no aggregate native-platform claim, and all workspace gates succeed.
