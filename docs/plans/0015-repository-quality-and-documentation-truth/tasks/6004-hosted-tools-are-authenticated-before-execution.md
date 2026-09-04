---
id: hosted-tools-are-authenticated-before-execution
title: "Hosted Tools Are Authenticated Before Execution"
workstream: "0060"
kind: task
depends_on: []
gated: false
touches:
  - crates/slingshot-development/tests/pinned_tool_installation.rs
  - crates/slingshot-development/tests/fixtures/pinned-tool-installation/manifests.jsonl
  - scripts/install_pinned_repository_tools
  - scripts/build_release_notes
  - support/repository-tools.toml
status: planned
merged_as: ""
---
# Hosted Tools Are Authenticated Before Execution

ShellCheck and the provider client are downloaded and extracted without a digest or signature check, then trusted to report their own expected versions. Release-note assembly separately trusts or installs a hard-coded `git-cliff` version. A program's output cannot authenticate the bytes already executing it.

**Steps:**

1. Put every executable acquired or accepted by hosted quality and release automation, including `git-cliff`, under one manifest authority. Require each consumer to use it and refuse an undeclared or separately hard-coded version.
2. Record an immutable digest or equivalently strong release provenance for each downloaded archive on each installer platform, separate from the version used to construct its URL.
3. Verify the still-archived bytes before extraction, installation, or execution. A byte mutation, archive substitution, missing identity, or identity for another platform installs and executes nothing.
4. Retain post-install version checks only as compatibility assertions. Refuse a counterfeit ambient `git-cliff` even when it prints the pinned version, and prove each hosted job authenticates a tool before its first use.

- **Done when:** every downloaded or ambient quality/release executable has one manifest-owned version and immutable source identity, corrupt/substituted/counterfeit fixtures execute and install nothing, and workflow assertions prove authentication precedes use.
