---
id: daemon-runtime-ownership
title: "Daemon Runtime Ownership"
workstream: "0003"
kind: task
depends_on:
  - platform-runtime-contract
gated: false
touches:
  - crates/slingshot-daemon/src/runtime_namespace.rs
  - crates/slingshot-daemon/src/ownership.rs
  - crates/slingshot-daemon/tests/runtime_ownership.rs
status: done
merged_as: "4ef6deed8be074d47ba1292ebcd42a675ad1bdb2"
---
# Daemon Runtime Ownership

A target namespace has one authority: the process holding its operating-system lock. Readiness and process records remain diagnostics and cannot manufacture ownership.

**Steps:**

1. Write namespace fixtures for distinct, visually similar, maximum-length, and invalid profile and environment values, including an injected temporary runtime root.
2. Derive the manifest-bounded Unix-domain-socket path or Windows named-pipe name, distinct owner-lock and startup-election-lock identities, plus an ephemeral readiness directory from canonical profile and environment values and the exact foundation-contract digest algorithm; expose display values separately from platform endpoint/lock identifiers.
3. Implement the supported platform daemon owner lock, owner-only namespace permissions or access control list, atomic readiness record, diagnostic process identifier, and one cryptographically random readiness nonce with the exact manifest length. Stale cleanup occurs only after exclusive owner-lock acquisition proves the prior owner is absent. Expose the separate startup-election identity to explicit-start clients without letting it authorize owner cleanup or daemon lifetime ownership.
4. Keep the lock handle and nonce alive in one ownership value. Its stop-authority method compares a request only with that exact live nonce; its drop removes only readiness data belonging to the same nonce and never unlinks the persistent lock file. A numeric process identifier, process-name lookup, or prior readiness record is never stop or signal authority.

**Tests:**

- Equal target values produce equal namespaces across processes; distinct pairs, including delimiter-ambiguous pairs, produce different namespaces.
- One owner succeeds and every contender receives a typed already-owned result carrying bounded diagnostic evidence.
- Owner-lock and startup-election-lock identities cannot alias or substitute for one another; abrupt election-client exit releases election only and cannot disturb a live owner.
- A stale process or readiness record without a held lock is recovered; a forged stale record cannot displace a live lock holder.
- Dropping one owner cannot remove a newer owner's nonce-mismatched readiness record.
- A stale nonce returns `stale_daemon_instance` and cannot stop, remove readiness for, or otherwise affect a replacement owner even when a fixture reuses the same numeric process identifier.
- All tests use injected temporary roots and leave the real user runtime and configuration directories untouched.

- **Done when:** `cargo test -p slingshot-daemon --test runtime_ownership` proves exclusive target ownership, collision-resistant manifest-bounded namespaces, current-user isolation, exact-length nonce authority, stale-nonce-safe cleanup, and stale-record recovery without treating a process identifier as authority.
