---
id: the-hosted-gate-is-given-what-it-verifies
title: "The Hosted Gate Is Given What It Verifies"
workstream: "0053"
kind: task
depends_on: []
gated: false
touches:
  - crates/slingshot-development/tests/pinned_tool_installation.rs
  - crates/slingshot-development/tests/fixtures/pinned-tool-installation/manifests.jsonl
  - scripts/install_pinned_repository_tools
status: planned
merged_as: ""
---
# The Hosted Gate Is Given What It Verifies

The gate refuses to run until every pinned tool reports its version. A machine that starts without them is the normal case for a hosted run, so the installer is as much a part of the gate as the checks are, and a tool the manifest names that the installer does not handle is a gate that refuses on a runner and nowhere else.

**Steps:**

1. Require every tool the manifest names to be one the installer installs, so a tool added to one and not the other is refused here rather than discovered on a runner.
2. Require the installer to read every version from the manifest: a version written into the installer a second time is refused.
3. Require the installer to verify each tool after installing it with the same check string the gate uses, and to refuse when a tool reports a version other than the pinned one.
4. Require the hosted gate job to install before it verifies, by reading the workflow rather than by trusting its step names.

- **Done when:** every tool the manifest names is one the installer handles and verifies, no version appears in the installer, and the hosted gate job installs before the stage that refuses without them.
