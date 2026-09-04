---
id: the-decision-inside-the-boundary
title: "The Decision, Inside The Boundary"
workstream: "0058"
kind: task
depends_on: []
gated: false
touches:
  - crates/slingshot-development/src/release_acceptance.rs
  - crates/slingshot-development/src/main.rs
  - crates/slingshot-development/tests/release_acceptance.rs
  - crates/slingshot-development/tests/fixtures/release-acceptance/decisions.jsonl
status: planned
merged_as: ""
---
# The Decision, Inside The Boundary

The release asks for a decision and receives nothing, because the command that makes it was never written. The document it must produce is already specified, and already verified by a command that exists.

**Steps:**

1. Run each gate the acceptance covers, in the order the manifest records them, inside the container and with only what the container is given.
2. Record what each gate concluded, and make a gate that refused decide the revision: unreleasable, naming which gate and why.
3. Write the manifest the verifier reads, and require the verifier to accept it - the document was specified before its producer, so the producer is held to the specification rather than the other way round.
4. Prove a gate that could not run at all is not recorded as one that held.

- **Done when:** a run whose every gate holds produces a manifest the existing verifier accepts, and a run with any gate refusing produces one it refuses, naming that gate.
