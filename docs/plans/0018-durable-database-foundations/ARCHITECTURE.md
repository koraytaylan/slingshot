# Plan 0018 — Durable Database Foundations

## Architectural boundary

An installation-state coordinator owns one pinned root handle and a process-shared lock covering complete enumeration, stable bounded read, classification, and durable replace. Names are resolved relative to that handle without following links. Publication uses a unique private stage, file sync, atomic replacement under the lock, and directory sync; recovery accepts only a complete old or new record.

Database open has two phases. A read-only, no-mutation compatibility inspection verifies the pinned ordinary file and schema version. Only an accepted database is reopened through the enforced connection factory, configured before general SQLite initialization where required, and then migrated. No caller receives an unrestricted `rusqlite::Connection`.

The connection factory is the sole SQL and file-object authority. It combines an SQLite authorizer with a closed typed statement surface and a real VFS/open mediation layer or demonstrably equivalent pinned-directory mechanism. It rejects unlisted SQL actions, PRAGMAs, ATTACH, extensions, and transient objects. Exact build/config identities are checked once before connections exist. Physical accounting includes the main database, WAL, shared memory, journals, and any explicitly allowed temporary object.

## What proves what

Two-process first-start tests and barrier races prove one installation identity. Link, owner, mode/ACL, link-count, replacement, unreadable enumeration, oversize, and injected sync failures prove refusals leave unrelated targets and the prior record unchanged.

SQLite runtime canaries attempt every prohibited action through real connections and observe authorizer/open refusal plus an unchanged directory. Fault tests interrupt compatibility, migration, transaction, WAL, checkpoint, and sync boundaries; reopening yields the old accepted schema or the fully committed new state. A future-schema fixture is compared byte-for-byte with all metadata and directory entries before and after refusal.

## What stays outside

Typed repositories remain free to evolve in later plans, but they may do so only through this connection factory.
