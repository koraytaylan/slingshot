---
id: installation-state-is-one-locked-ledger
title: "Installation State Is One Locked Ledger"
workstream: "0069"
kind: task
depends_on: []
gated: false
touches:
  - crates/slingshot-storage/src/installation_state.rs
  - crates/slingshot-storage/tests/installation_state.rs
status: planned
merged_as: ""
---
# Installation State Is One Locked Ledger

Installation identity follows path links, reads without a bound, uses a predictable truncating stage, ignores enumeration failures, and declares but never takes its lock. Concurrent first starts or same-user substitutions can therefore change identity or damage another file.

**Steps:**

1. Pin a verified owner-private state-root handle and take one cross-process lock spanning complete directory enumeration, read/classify, first creation, and replacement; make every enumeration error fail closed.
2. Open record, lock, and unique stage relative to the root without following links. Require an ordinary single-link object, exact owner and Unix mode or Windows ACL, stable pre/post-read identity, and a named byte limit before JSON parsing.
3. Publish canonical records with create-new stage, file sync, atomic transition, and directory sync. On every fault or crash, recovery admits exactly the prior complete record or the new complete record and never follows or truncates an unrelated target.
4. Race two real processes on first start and replacement; cover root/record/stage/lock symlinks, hardlinks, owner, permissions/ACL, oversize, replacement during read, and unreadable directory entries.

- **Done when:** all processes converge on one durable installation identity and ledger, every filesystem substitution or scan error refuses without changing any target, and crash recovery yields only a complete old or new canonical record.
