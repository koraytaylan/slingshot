---
id: what-the-decision-is-told
title: "What The Decision Is Told About Its Run"
workstream: "0059"
kind: task
depends_on: ["the-decision-inside-the-boundary"]
gated: false
touches:
  - support/release-acceptance-container.toml
  - scripts/release_acceptance
  - .github/workflows/release.yml
  - crates/slingshot-development/tests/release_acceptance.rs
status: completed
merged_as: "48b81cb"
---
# What The Decision Is Told About Its Run

The manifest binds the revision, the tree, and the provider run. A container told nothing about its run cannot state any of them, and the environment it admits is closed on purpose, so each one has to be let in deliberately.

The revision and the tree are the host's to read from the checkout it is accepting. Which provider run this is, is the provider's own statement about itself, and this repository only ever hears it through the variable a workflow job maps it into. The acceptance job maps no such variable, so the workflow is in this footprint: without it the run would be told nothing and would refuse, which is the correct behaviour arriving at the wrong time.

**Steps:**

1. Admit each value the manifest binds through the declared environment, and record beside it in the contract why it is admitted.
2. Refuse a run that is told nothing: a decision that guessed its revision would be a decision about something else.
3. Prove nothing is inferred from the container's surroundings, so a container cannot be persuaded it is a run it is not.

- **Done when:** the manifest's bound values all arrive through the declared environment, a run missing any of them refuses, and no value is read from anywhere else.
