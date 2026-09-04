---
id: sqlite-file-policy-mediates-real-opens
title: "SQLite File Policy Mediates Real Opens"
workstream: "0070"
kind: task
depends_on: ["sqlite-is-configured-before-the-first-connection"]
gated: false
touches:
  - crates/slingshot-storage/src/database.rs
  - crates/slingshot-storage/src/sqlite_vfs.rs
  - crates/slingshot-storage/tests/migrations.rs
status: planned
merged_as: ""
---
# SQLite File Policy Mediates Real Opens

The object whitelist can classify names in a unit test, but SQLite opens the default VFS directly. Parent directories and database objects are not pinned to the verified state root, and unlisted transient or attached files are not intercepted.

**Steps:**

1. Register and select a genuinely enforcing VFS or equivalent pinned-directory open layer in the sole connection factory; no caller may select the default or another VFS.
2. Resolve main, WAL, and shared-memory objects relative to a retained verified directory handle without following links, and require owner, mode/ACL, type, link count, and stable identity.
3. Reject path escape, URI alternate names, attachment targets, temp/journal files, device/special files, and every object outside the closed permitted inventory at the actual open callback.
4. Force each SQLite object/open mode with real canaries and race root/object replacements; assert prohibited attempts refuse and the complete directory inventory remains unchanged.

- **Done when:** every real SQLite file open is relative to the pinned verified database root and admitted by the closed object policy, and no path spelling, link, replacement race, attachment, or transient mode can create or access an undeclared object.
