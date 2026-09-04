---
id: the-second-time-or-its-absence
title: "The Second Time, Or Its Absence"
workstream: "0057"
kind: task
depends_on: ["one-handle-one-identity"]
gated: false
touches:
  - crates/slingshot-configuration/src/credential_filesystem.rs
  - crates/slingshot-configuration/tests/credential_filesystem_windows.rs
  - docs/CONFIGURATION.md
status: planned
merged_as: ""
---
# The Second Time, Or Its Absence

The evidence tuple carries two times so that "unchanged" means unchanged rather than "the same length". One of them has no stable source on this row, and the interface that would report it is offered by no version of the declared capability.

**Steps:**

1. Decide what the row reports: a second time it can actually produce and that moves when the contract needs it to, or one time and an explicit statement that the row carries one.
2. Record the decision where a reader meets it, in product documentation rather than only in a comment, because a tuple whose meaning differs by row is a thing a reader has to be told.
3. Pin what each row reports in the fixtures, so a row quietly reporting a different shape is refused.
4. Prove the cases the times exist for on this row: content changed in place, and the object replaced atomically.

- **Done when:** in-place mutation and atomic replacement are each refused on this row by the evidence tuple alone, and the fixtures pin what every row reports.
