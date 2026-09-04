---
id: one-handle-one-identity
title: "One Handle, One Identity"
workstream: "0056"
kind: task
depends_on: []
gated: false
touches:
  - crates/slingshot-configuration/src/credential_filesystem.rs
  - crates/slingshot-configuration/tests/credential_filesystem_windows.rs
status: planned
merged_as: ""
---
# One Handle, One Identity

Three readers describe one credential: the one that reads its bytes, the one that decides who may hold rights on it, and the one that reports its identity. They must describe the same object, and today the third one cannot be called at all on a released compiler.

**Steps:**

1. Restore the row: the matrix entry, the automation authority row, the workflow matrices, and the four capability rows with their consumers and manifest entries. They left together when the release stopped claiming the row, and an assertion holds the inventory equal to the matrix, so they return together.
2. Choose the handle the row uses, and take all three readings from it. A design in which the security decision cannot be made on the chosen handle is refused rather than worked around.
3. Read the volume, the volume-scoped identifier, the link count, and the reparse evidence through the declared identity capability rather than through an unstable interface.
4. Replace the object between the identity reading and the content reading and require the read to refuse, on the row itself, so sameness is demonstrated rather than assumed.
5. Keep the refusals the row already makes: a reparse point, a hard-linked credential, and an object owned by another principal each refuse for the reason they refuse for today.

- **Done when:** the row compiles on the pinned released compiler, and an object replaced between two of the three readings is refused by the row itself.
