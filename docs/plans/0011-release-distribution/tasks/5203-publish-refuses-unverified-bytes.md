---
id: publish-refuses-unverified-bytes
title: "Publish Refuses Unverified Bytes"
workstream: "0052"
kind: task
depends_on: ["release-version-agreement"]
gated: false
touches:
  - crates/slingshot-development/tests/publish_release_refusals.rs
  - crates/slingshot-development/tests/fixtures/publish-release/runs.jsonl
  - scripts/publish_release
status: planned
merged_as: ""
---
# Publish Refuses Unverified Bytes

The publisher authenticates every archive before it uploads anything. That ordering is the whole safety of the step, and a message saying it happened is not evidence that nothing was uploaded.

**Steps:**

1. Build a recording stand-in for the provider client that writes every invocation it receives and performs none of them, and place it ahead of the real one for the duration of each case.
2. Commit one row per downloaded run: every archive authenticating, one archive whose attestation does not authenticate, the notes absent, and no archive present at all.
3. Drive `scripts/publish_release` against each row and require the recording to be empty for every refusal, and to hold exactly one publish naming every archive and every attestation bundle for the accepted row.
4. Require the refusal to happen before the first upload rather than between uploads, by placing the unauthenticated archive after an authenticating one in the same run.

- **Done when:** each refusal case leaves the recording empty, and the accepted case records one publish carrying every archive and every bundle.
