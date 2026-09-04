---
id: every-supported-row-gates-the-change
title: "Every Supported Row Gates The Change"
workstream: "0060"
kind: task
depends_on: []
gated: false
touches:
  - .github/workflows/platform-runtime.yml
  - scripts/platform_quality
  - crates/slingshot-development/tests/github_workflow_contract.rs
  - crates/slingshot-development/tests/fixtures/github-workflow-contract/workflows.jsonl
status: planned
merged_as: ""
---
# Every Supported Row Gates The Change

macOS is supported, but its only hosted execution happens after push and runs one integration target. A pull request can therefore merge code that does not compile in another macOS-gated target or test.

**Steps:**

1. Add one argument-free repository-local native check that at least compiles the workspace with all targets/features and runs the complete declared OS-sensitive inventory without network access or source mutation.
2. Run that exact check for every row derived from `support/platforms.toml` on both push and pull request, after validating the row's automation authority.
3. Extend parsed workflow fixtures to refuse a missing pull-request trigger, a missing supported row, a hard-coded extra row, a one-test command, fail-fast cancellation, or a native job that does not invoke the repository-local check.
4. Add a macOS-only sentinel outside `platform_runtime_contract` and prove it is compiled or executed by the native pull-request job.

- **Done when:** every supported row runs the same complete native check before merge, and removing the pull-request trigger, any row, or any part of the check makes the workflow contract fail.
