---
id: sqlite-schema-and-migrations
title: "SQLite Schema And Migrations"
workstream: "0015"
kind: task
depends_on:
  - operation-lifecycle
  - canonical-command-fingerprint
  - persistent-installation-identifier
gated: false
touches:
  - crates/slingshot-storage/src/database.rs
  - crates/slingshot-storage/src/sqlite_statement_inventory.rs
  - crates/slingshot-storage/src/sqlite_vfs.rs
  - "crates/slingshot-storage/migrations/**"
  - crates/slingshot-storage/tests/migrations.rs
  - "crates/slingshot-storage/tests/fixtures/migrations/**"
status: done
merged_as: "fd2ca6d34485d41ab233d9f88290d2470b0cd051"
---
# SQLite Schema And Migrations

The daemon needs a recoverable schema before any service writes operations. This task establishes transactional migration and open behavior without adding repository methods.

**Steps:**

1. Author empty, current principal-bound target, each-supported-old-version, interrupted-migration, corrupt, missing/malformed opaque-principal identity, stale daemon-runtime digest, newer-than-binary database, exact/over database/WAL/shared-memory/replacement objects, WAL header/frame arithmetic, pinned-reader write churn, interrupted checkpoint, wrong SQLite source/build/compile option, late statement-journal configuration, unknown built-in/delegating VFS, prohibited-open, ambient-temp canary, and changed/unknown SQL-inventory fixtures before migration code.
2. Define ordered governed Structured Query Language migrations for installation snapshot, exact `DaemonRuntimeContractDigest`, complete opaque `AuthorTargetIdentity`, its direct no-second-hash `AuthorTargetIdentityDigest` partition value, partitioned operation identifier, exact selected revision, revision-bound fingerprint/command, lifecycle/progress/conditional-recovery-evidence revisions, bounded progress, enqueue order, caller/workflow, retry observation/delay/UTC diagnostic facts, bounded target-and-selected-revision-bound recovery-resume receipts, result disposition, conditional terminal failure kind/disposition payload/metadata including authoritative remote success, timestamps, artifact slots, bounded target-partitioned maintenance-application receipts, and operation-free target-qualified maintenance-result associations carrying identifier/kind/reviewed-source/content digest/length/fixed media type/association revision/blob reference plus exactly one current-preview or application-receipt owner, with referential constraints. Raw Basic/Cloud principal, metascope, trust, profile-contract, and certificate fields are never schema columns; only Plan 0002's opaque target/revision values persist.
3. Add the exact listing index over target digest, descending enqueue sequence, and operation identifier, plus uniqueness keys that make every replay, artifact association, maintenance-result association, and child fact target-partitioned. Constraints permit at most one current unapplied preview per target, prevent a retired/missing application receipt from owning a result, and reject an association whose identifier cannot be rederived from its target/kind/source/content fields.
4. Give the single daemon process write-connection ownership and pin its exact SQLite source identifier, dependency build, and compile-option set. Before library initialization set `SQLITE_CONFIG_STMTJRNL_SPILL = -1`; require compile option `TEMP_STORE=3`; then verify `PRAGMA temp_store = MEMORY`, every manifest-owned page-size/page-count/write-transaction/busy setting, `foreign_keys = ON`, `journal_mode = WAL`, and `synchronous = FULL` on every connection. Keep every read transaction internal and enforce the manifest duration through the pinned SQLite progress/interrupt boundary before copying a result out.
5. Commit a closed inventory of every parameterized repository/migration statement and its bounded inputs, result, and independently reviewed query plan. Forbid dynamic SQL, `ATTACH`, `DETACH`, `VACUUM`, `VACUUM INTO`, temporary-schema objects, caller pragmas, and every unaccounted disk-materialization route. Require sorts, transient indexes, temporary databases, and materialized results to remain in bounded memory and statement journals never to spill.
6. Select the restrictive delegating VFS over the exact pinned platform local-filesystem VFS. Permit only active main/`-wal`/`-shm` plus one same-directory replacement-main object; reject all rollback/master/replacement sidecars, `TEMP_DB`, `TEMP_JOURNAL`, `TRANSIENT_DB`, `SUBJOURNAL`, `SUPER_JOURNAL`, `DELETEONCLOSE`, unknown, and outside-root opens. Build the disposable replacement with journaling off and exclusive locking under the same memory/no-spill rules; integrity-check, close, synchronize, and rename it before directory synchronization. Poison every ambient temporary path in tests.
7. Account database/WAL/shared-memory handle lengths independently of logical rows and calculate WAL length from its one 32-byte header plus 24-byte-header/page frames. Apply passive checkpoint at the exact write-page threshold; at the exact byte high-water stop new fact-producing writes so one maximum transaction cannot cross the cap, drain bounded readers, and attempt truncate recovery within the manifest duration. Busy/failed/late/still-over-bound recovery remains read-only `PersistentStorageBackpressure` without mutation. Startup validates and recover-checkpoints a legal WAL before readiness; migrations reserve the independently recomputed complete replacement formula and synchronize file/rename/directory order.
8. Refuse a newer schema, corruption, installation/digest mismatch, unsafe owner/type/link/permission state, physical-file limit, SQLite source/build/configuration/VFS/SQL-inventory/no-spill invariant, or setting mismatch without changing bytes; repeated current-version open is a no-op.

**Tests:**

- Empty and every supported old fixture migrate to the exact current principal-bound schema and preserve seeded rows; any supported identity-format migration has committed vectors and never guesses an absent principal. Reopen reconstructs every maintenance-result association and refuses dangling owner/blob, duplicate-current-preview, derivation, digest, length, media-type, or revision corruption unchanged.
- An injected failure rolls back the whole migration and succeeds cleanly on the next open.
- Current-schema open makes no schema write.
- Newer and corrupt fixtures are refused byte-for-byte unchanged.
- Every connection reports the exact manifest page/page-count/transaction/busy values plus foreign-key, write-ahead-log, full-synchronization, single-writer, memory-temp-store, no-statement-journal-spill, exact SQLite source/build, and restrictive-VFS settings; no client request can force or bypass a checkpoint.
- The closed SQL inventory admits no dynamic/prohibited statement or unbounded materialization. Faulting every ambient temporary location plus the VFS open oracle proves statement journals, sorts, transient indexes, materialized results, and temporary databases cannot create disk objects; any attempted unlisted open or unavailable proof refuses before readiness.
- A pinned reader ends at the exact duration boundary; independently generated 32-byte-header/24-byte-frame vectors prove repeated maximum write transactions reach high-water without crossing the WAL bound, refuse later writes, and recover only after bounded readers drain and a successful truncate checkpoint. Checkpoint busy/failure/restart preserves committed bytes and read-only diagnosis without inferring a physical bound from row counts.
- Crash checkpoints before and after schema commit, accepted-operation commit, passive checkpoint, truncate checkpoint, rename, and directory synchronization reopen to either the preceding complete state or the succeeding complete state.
- Active main/write-ahead-log/shared-memory, replacement main, and directory permissions are current-user-only; every rollback/master/temp/replacement sidecar or unsafe existing object fails closed.

- **Done when:** `cargo test -p slingshot-storage --test migrations` upgrades every digest/target/revision-bound fixture transactionally, proves exact pinned SQLite build/configuration/VFS/SQL/no-disk-transient invariants, independent WAL-frame and aggregate physical bounds, bounded readers, checkpoint/backpressure/restart behavior, exact whitelisted objects, permissions and crash ordering, and refuses every unverifiable or mismatched state unchanged, and all workspace gates succeed.
