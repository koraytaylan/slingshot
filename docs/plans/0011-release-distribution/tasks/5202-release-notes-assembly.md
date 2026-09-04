---
id: release-notes-assembly
title: "Release Notes Assembly"
workstream: "0052"
kind: task
depends_on: []
gated: false
touches:
  - crates/slingshot-development/tests/release_notes_assembly.rs
  - crates/slingshot-development/tests/fixtures/release-notes/history.jsonl
  - cliff.toml
status: planned
merged_as: ""
---
# Release Notes Assembly

The notes are the history grouped by what each commit touched. Which commits reach them is a claim about the configuration, and asserting it against this branch would make the assertion drift every time somebody commits.

**Steps:**

1. Commit a synthetic history holding one plan bookkeeping commit, one documentation commit that publishes a product reference, one feature and one fix under different scopes, and one commit under a scope no other commit uses.
2. Assemble notes over that history in both forms: named by a tag when a tag started the run, and named as unreleased when nothing did.
3. Require the bookkeeping commit to be absent, every other commit to be present exactly once under its own scope, and the scopes to appear in the order the configuration declares rather than alphabetically.

- **Done when:** a synthetic history containing both a bookkeeping and a product documentation commit yields notes holding exactly one of them, in both the tagged and the untagged form.
