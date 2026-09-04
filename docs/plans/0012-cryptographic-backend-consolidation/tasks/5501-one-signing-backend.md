---
id: one-signing-backend
title: "One Signing Backend"
workstream: "0055"
kind: task
depends_on: []
gated: false
touches:
  - crates/slingshot-agent-connection/src/token_assertion.rs
  - crates/slingshot-agent-connection/Cargo.toml
  - Cargo.toml
  - policy/workspace-capabilities.toml
status: planned
merged_as: ""
---
# One Signing Backend

The credential assertion is one signature over one encoded document with one algorithm. It is produced by a library whose only purpose here is that signature, and which brings a whole second cryptographic implementation to make it.

**Steps:**

1. Produce the signature against the backend the transports already use, reading the same key material from the same parsed credential, and encode the assertion exactly as it is encoded now.
2. Remove the assertion library and its backend from the workspace manifest and from the capability inventory, adding no row: the replacement is a capability this workspace already declares.
3. Require the committed assertion fixtures to be reproduced byte for byte for every pinned key and sampled instant, including the boundary instants the exchange contract names.
4. Require the exchange suite to accept the assertion unchanged, and the refusal cases to refuse for the reasons they already refuse for.

- **Done when:** every committed assertion fixture is reproduced byte for byte with the assertion library absent from the resolved graph.
