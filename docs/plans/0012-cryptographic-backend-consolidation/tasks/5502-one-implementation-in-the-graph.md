---
id: one-implementation-in-the-graph
title: "One Cryptographic Implementation In The Graph"
workstream: "0055"
kind: task
depends_on: ["one-signing-backend"]
gated: false
touches:
  - crates/slingshot-development/tests/cryptographic_backend_inventory.rs
  - crates/slingshot-development/tests/fixtures/cryptographic-backend-inventory/graphs.jsonl
status: planned
merged_as: ""
---
# One Cryptographic Implementation In The Graph

Removing the duplicate is worth little if the next dependency's default brings another one back. The first arrived that way, unremarked, and stayed until a runner that lacked an assembler refused to build it.

**Steps:**

1. Read the resolved graph rather than the manifest, because a manifest says what was asked for and a graph says what a build links.
2. Name the implementations this workspace admits, and refuse any other, reporting which dependency pulled it and by which path.
3. Commit graphs that reproduce each way a second implementation enters: a direct dependency, a transitive default feature, and a backend feature swapped on an existing dependency. Each is refused with the pulling dependency named.
4. Require the committed graph to carry exactly one, on every supported target.

- **Done when:** a resolved graph carrying a second cryptographic implementation is refused with the name of the dependency that pulled it, and the committed graph carries exactly one on every supported target.
