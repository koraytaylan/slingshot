---
id: publication-requires-one-closed-release
title: "Publication Requires One Closed Release"
workstream: "0060"
kind: task
depends_on: ["the-release-decision-binds-its-evidence"]
gated: false
touches:
  - crates/slingshot-development/tests/publish_release_refusals.rs
  - crates/slingshot-development/tests/fixtures/publish-release/runs.jsonl
  - scripts/publish_release
status: complete
merged_as: "pending"
---
# Publication Requires One Closed Release

The publisher authenticates every archive it finds, but "every" can be one supported row. It does not require the acceptance decision, a clean checkout at the tag, or agreement between the local and remote peeled tag. Completeness and revision authority must be part of the same preflight as provenance.

**Steps:**

1. Require a clean verifier checkout whose `HEAD`, local peeled tag, remote peeled tag, and accepted source commit are the same full commit before reading policy or evidence. Apply the same rule to release creation and editing.
2. Derive the exact release rows from `support/platforms.toml`, then cover the complete set, every single missing row, an unexpected or duplicate row, bad attestation, absent notes or acceptance, decision about another commit, dirty policy, wrong `HEAD`, moved local tag, moved remote tag, and empty run.
3. Independently verify the acceptance directory and every row's archive/evidence/attestation before the first provider mutation. Never let a row's own stated digest establish completeness or identity for the run.
4. Drive the real publisher against a recording provider client. Put each bad or missing item last in enumeration order and require no create, edit, or upload for every refusal.

- **Done when:** only a clean checkout at the identical local-and-remote tag, an accepted decision for that commit, and exactly one authentic archive-and-attestation pair for every supported row can mutate a release; every drift, incomplete, extra, duplicate, or invalid case records no provider action.
