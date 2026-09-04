---
id: download-publication-never-replaces
title: "Download Publication Never Replaces"
workstream: "0067"
kind: task
depends_on: []
gated: false
touches:
  - crates/slingshot-command-line/src/artifact_download.rs
  - crates/slingshot-command-line/src/artifact_staging_lock.rs
  - crates/slingshot-command-line/src/platform_runtime/**
  - crates/slingshot-command-line/tests/operation_observation.rs
status: completed
merged_as: ""
---
# Download Publication Never Replaces

The download path promises never to overwrite a user destination, but checks absence and then calls a primitive that replaces a newly arrived destination on Unix. Its predictable lock path can also follow a preplaced link.

**Steps:**

1. Implement platform-specific atomic no-replace publication behind one typed adapter, using directory-relative handles and refusing unsupported or ambiguous fallback behavior.
2. Create stage and lock objects uniquely, privately, and without following links; verify owner, mode or ACL, link count, type, and stable identity on the retained handle before use.
3. Synchronize completed bytes and the containing directory before reporting success, and clean only this invocation's authenticated stage after a loss or failure.
4. Inject a regular file and symlink between validation and publication, preplace stage/lock symlinks and hardlinks, widen permissions, and run two publishers concurrently. Require the user's destination and every redirection target to remain unchanged, with exactly one winner.

- **Done when:** no race, link, sidecar substitution, or competing publisher can replace or truncate a destination, and a reported success names one durably published file with the verified bytes.
