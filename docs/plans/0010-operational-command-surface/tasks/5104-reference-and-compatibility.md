---
id: reference-and-compatibility
title: "Reference and Compatibility"
workstream: "0051"
kind: task
depends_on:
  - command-line-surface
gated: false
touches:
  - docs/COMMANDS.md
  - docs/MODEL_CONTEXT_PROTOCOL.md
  - docs/AGENT_PROTOCOL.md
  - docs/DOCUMENTATION_REVIEW.md
  - crates/slingshot-command-line/src/live_adobe_experience_manager.rs
  - README.md
  - ARCHITECTURE.md
  - crates/slingshot-development/tests/fixtures/protocol-compatibility/snapshot.json
  - examples/finite-state-machine/slingshot.handlers.template.json
status: done
merged_as: "2ca4b05987bed0bc33874600e4d7939451e46d86"
---
# Reference and Compatibility

Every document that says what this build offers currently says twelve. This task makes them say sixty-four by regenerating what is generated and rewriting what a person wrote, and it makes the compatibility snapshot record the growth as growth rather than as drift.

**Steps:**

1. Regenerate the generated blocks of `docs/COMMANDS.md` from the registry and the option table: the registry command table, the local-leaf table, and the option tables the reference test already renders and compares.
2. Update `docs/MODEL_CONTEXT_PROTOCOL.md` where it states how many tools the server publishes and which operation keys they take, keeping it a description of this commit rather than a plan for one.
3. Update `docs/AGENT_PROTOCOL.md` where it states what the daemon holds an author to, so the contract an agent implements names the families it now has to implement.
4. Update the two prose statements that count commands: the README's live-author paragraph, which says nine, and this repository's architecture note about what is and is not here.
5. Refresh the protocol compatibility snapshot to the sixty-four-row registry, and keep the assertion that made it worth having: an existing row that changed its version, its limits digest, or either schema digest is a compatibility break and fails, while a new row is growth and passes. Say plainly why this refresh moves every existing row's digests as well as adding new ones: the byte contract gained the comparators the new listings order by, every role schema carries that contract's digest as an annotation by design, and so every schema digest moved. Nothing has consumed the old ones - no package is published and no release artifact exists - which is what makes a refresh the honest answer rather than a version bump on sixty-four commands whose meaning did not change.
6. Add the new command names to the finite-state-machine handler template, so a workflow that dispatches a registry command can dispatch the ones this plan added.

**Tests:**

- Every generated block in the reference equals what the build renders from the registry and the option table, with no row that the build does not publish and no published row that is missing.
- No product document contains an unfinished-work marker or a planning heading, and every repository path and link a document names resolves to something committed.
- The compatibility snapshot accepts the sixty-four-row registry, and a mutated fixture that alters an existing row's version or either digest fails the assertion.
- The handler template parses under the handler validation the workflow gate already runs, and every command name it dispatches is one the registry publishes.
- The documentation review checklist records this plan's four review subjects as reviewed rather than as decided by a checker.

- **Done when:** `scripts/quality` passes with every reference table regenerated from the registry, the snapshot recording sixty-four rows, the mutation test still failing an altered existing row, and no product document claiming a command surface the build does not have.
