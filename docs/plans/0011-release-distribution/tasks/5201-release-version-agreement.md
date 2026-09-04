---
id: release-version-agreement
title: "Release Version Agreement"
workstream: "0052"
kind: task
depends_on: []
gated: false
touches:
  - crates/slingshot-development/tests/release_version_agreement.rs
  - crates/slingshot-development/tests/fixtures/release-version/runs.jsonl
status: planned
merged_as: ""
---
# Release Version Agreement

A tag and a manifest are two places the same version is written, so they can disagree quietly and nothing downstream would notice. The refusal exists; nothing proves it.

**Steps:**

1. Commit one fixture row per run the provider can report: a tag naming the declared version, a tag naming another version, a tag without the release prefix, and a run the provider started by hand with no tag at all. Each row records the expected exit and whether anything is written.
2. Drive `scripts/verify_release_version` against every row through its real environment variables, requiring the accepted row to succeed and each refused row to exit nonzero naming the field that disagreed.
3. Prove both sides of the prefix boundary: the exact prefix accepted and one character short of it refused.

- **Done when:** every committed row's recorded outcome is reproduced by the real script, and a tag naming a version the workspace does not declare cannot reach a build.
