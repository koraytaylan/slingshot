---
id: process-identity
title: "Process Identity"
workstream: "0040"
kind: task
depends_on:
  - operational-contract-limits
gated: false
touches:
  - crates/slingshot-domain/src/command/process_identity.rs
  - crates/slingshot-domain/src/command/mod.rs
  - crates/slingshot-domain/tests/process_identity.rs
status: planned
merged_as: ""
---
# Process Identity

Workflow models, workflow instances, work items, and Sling jobs are addressed by values an author mints. A model and an instance are spelled as repository paths in every deployment anyone has seen and are not required to be, so this task treats them as bounded opaque values rather than asserting a shape the author never promised - while a job topic does have a grammar Sling enforces, and gets one.

**Steps:**

1. Implement `WorkflowModelIdentifier`, `WorkflowInstanceIdentifier`, and `WorkItemIdentifier` as bounded non-empty opaque values at their own named limits, refusing controls and edge spaces and requiring normalization form C.
2. Implement `SlingJobTopic` under the solidus-separated token grammar: one or more non-empty tokens over letters, digits, hyphen-minus, low line, and full stop, no leading or trailing solidus, bounded by `MAXIMUM_SLING_JOB_TOPIC_BYTES`.
3. Implement `SlingJobIdentifier` and `SlingJobQueueName` as bounded non-empty values refusing controls and edge spaces.
4. Decide nothing about existence: no value here says whether an identifier names something an author has, and none infers a repository path from one.
5. Implement the closed `WorkflowInstanceState`, `SlingJobState`, and `SlingJobQueueState` enumerations the architecture names, each serialized in snake case, and a validated non-empty ascending `RequestedStates` set for each of the first two, bounded by its named state limit and refusing a repeat.

**Tests:**

- Each identifier accepts ordinary spellings and refuses empty, oversized by one byte, control-bearing, space-edged, and non-normalized forms.
- The topic accepts single and multiple segments, refuses an empty segment, a leading or trailing solidus, and a character outside its alphabet.
- Each closed enumeration round-trips and refuses an unknown spelling.
- A requested state set refuses empty, refuses a repeat, refuses a descending order, and accepts exactly the ascending distinct set, with the item bound proved on both sides.

- **Done when:** `cargo test -p slingshot-domain --test process_identity` proves every grammar, both sides of every bound, the closed enumerations, and the ascending non-empty state sets.
